# CODEX P1 — B6 benchmark overload exposes ledger/log hot-path issue

Time: 2026-09-16 ICT.  
Scope: current real worktree plus isolated benchmark-harness patch candidate smoke.

## Verdict

The previous B6 Bash concurrency bug is fixed, but a deeper benchmark/architecture issue remains before any SOTA claim:

- Current B6 is unbounded saturation against a ~0ms mock.
- That can drive hundreds of thousands of requests/sec locally, far above the release threshold of 8,000 rps.
- The router then fills ledger queues and logs one `ERROR` per dropped usage event from the hot path.
- If stdout/stderr is a pipe or slow log collector, that logging can block the router and make B6 artifact meaningless.

Fix this once. Do not hide it by setting `RUST_LOG=off` in the benchmark.

## Evidence

Current ledger hot path, `src/ledger/mod.rs:29-43`:

```rust
pub fn try_record(&self, event: UsageEvent) {
    match self.primary.try_send(event) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(ev)) => match self.overflow.try_send(ev) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_) | mpsc::error::TrySendError::Closed(_)) => {
                metrics::counter!("router_ledger_dropped_total").increment(1);
                tracing::error!("ledger: primary + overflow full, dropping usage event");
            }
        },
        Err(mpsc::error::TrySendError::Closed(ev)) => {
            if self.overflow.try_send(ev).is_err() {
                metrics::counter!("router_ledger_dropped_total").increment(1);
                tracing::error!(
                    "ledger: primary closed and overflow unavailable, dropping usage event"
                );
            }
        }
    }
}
```

Current queue sizing, `src/main.rs:142-145`:

```rust
let (primary_tx, primary_rx) = mpsc::channel(8192);
let (overflow_tx, overflow_rx) = mpsc::channel(16_384);
let ledger = LedgerSink::new(primary_tx, overflow_tx);
let writer = LedgerWriter::new(100, 1);
```

Current B6, `benchmarks/gate.sh:58-61`:

```bash
echo "== B6: throughput bão hoà 1k (conc 200)"
read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json" 200)
gate B6 "throughput rps (min)" "$(python3 -c "print(-1*$rps)")" "$(python3 -c "print(-1*$(thr "['B6']['min_rps_4core']"))")"
gate B6 "non200" "$rn" "$(thr "['B6']['max_non200']")"
```

Isolated harness smoke with B6 fixed to explicit conc=200 but still unbounded saturation produced:

```text
B1/B2/B3 and B4 all PASS
== B6: throughput bão hoà 1k (conc 200)
router-sat.json:
  summary.requestsPerSec: 66.53
  statusCodeDistribution: {}
  errorDistribution: {"aborted due to deadline": 200}
then gate generated: python3 -c 'print(-1*)' SyntaxError because rps was empty/null
router output flooded:
  ERROR ledger: primary + overflow full, dropping usage event
```

Earlier 1s smoke produced `router-sat.json` with the same shape:

```json
{
  "statusCodeDistribution": {},
  "errorDistribution": {"aborted due to deadline": 200},
  "summary": {"requestsPerSec": 199.11667185037055, "successRate": null}
}
```

This is not a valid throughput artifact.

## Root causes

### Root cause 1 — per-drop logging is on the hot path

On ledger overflow, the hot path increments a metric and writes an error log for every dropped usage event. At overload, this becomes a log storm. In Docker/K8s/systemd, stdout may apply backpressure; with `Popen(..., stdout=PIPE)` it definitely can. That turns accounting degradation into request-path latency/hangs.

### Root cause 2 — B6 uses unlimited saturation while the spec says open-loop gates

`BENCHMARK.md` principle 3 says open-loop fixed rate. B6 threshold is `min_rps_4core = 8000`, but current gate does not use a target rate. Against a zero-latency mock, it can generate hundreds of thousands of attempts/sec. That tests “can ledger/log survive absurd overload”, not “can router sustain ≥8k rps with features enabled”.

### Root cause 3 — gate does not validate `oha` output before using it

When `oha` writes null p50/p99 or no status distribution, `jq`/Bash can pass empty fields into Python and produce syntax errors. The gate should fail with a clear message and preserve raw artifact.

## Required repair shape

### A. Fix ledger drop logging without adding Redis

Keep hot path non-blocking and Postgres-only.

Recommended minimal change:

- Keep `router_ledger_dropped_total` metric increment.
- Replace per-event `tracing::error!` with a rate-limited log, max once per second, shared by the `LedgerSink` clone.
- Do not allocate strings per drop.

Patch shape:

```rust
pub struct LedgerSink {
    primary: mpsc::Sender<UsageEvent>,
    overflow: mpsc::Sender<UsageEvent>,
    last_drop_log_sec: Arc<AtomicU64>,
}

fn record_drop(&self, reason: &'static str) {
    metrics::counter!("router_ledger_dropped_total").increment(1);
    let now = now_secs() as u64;
    let last = self.last_drop_log_sec.load(Ordering::Relaxed);
    if now > last
        && self.last_drop_log_sec
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        tracing::error!(reason, "ledger dropping usage events; suppressing repeated drop logs");
    }
}
```

Then `try_record` calls `self.record_drop("primary_overflow_full")` or `self.record_drop("primary_closed")`.

Do not make `try_record` await, write DB, write file, or read env.

### B. Make B6 a valid release gate

Use a fixed target rate for pass/fail, plus optional saturation report if wanted.

Recommended gate behavior:

- `B6_TARGET_RPS=${B6_TARGET_RPS:-$(thr "['B6']['min_rps_4core']")}`.
- Run oha with `-q "$B6_TARGET_RPS" -c 200` for the pass/fail gate.
- Pass if actual rps is within a reasonable floor, e.g. `actual >= 0.98 * B6_TARGET_RPS`, and `non200 == 0`.
- If you still want max throughput, write a separate informational `router-sat-unbounded.json`, not a release gate until ledger capacity policy is specified.

This follows the benchmark spec’s open-loop principle and keeps budget/ledger enabled.

### C. Validate oha JSON before Python arithmetic

After every `oha`, parse raw JSON and fail early if:

- `.latencyPercentiles.p50` or `.latencyPercentiles.p99` is null,
- `.summary.requestsPerSec` is null,
- `.errorDistribution` is non-empty,
- status distribution is empty,
- router `non200` exceeds threshold.

Print the artifact path and `errorDistribution` on failure.

### D. Make benchmark seed budget realistic for high-rate mock

Benchmark seed must keep budget enabled but not become the bottleneck. Use a huge finite budget, e.g.:

```json
{"period":"month","max_tokens":9000000000000000000,"per_model":{}}
```

Do not set budget to `NULL` for official benchmark, because `BENCHMARK.md` requires budget enabled.

## Acceptance checks

1. Re-run code gates:

```bash
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo clippy --all-targets -- -D warnings
DATABASE_URL=<fresh pg> ADMIN_MASTER_KEY=test-admin cargo test --all-targets
```

2. Re-run benchmark smoke:

```bash
make bench-gate-smoke
```

Expected:

- B1/B2/B3 PASS.
- B4 PASS.
- B6 target-rate gate PASS or clear threshold failure with valid `router-b6-*.json`.
- No Python `print(-1*)` syntax errors.
- No per-request ledger drop log flood.

3. Inspect metrics/logs after B6:

- `router_ledger_dropped_total` should be 0 at target 8k if DB/ledger can keep up, or nonzero with rate-limited logs if truly overloaded.
- If drops occur at the official 8k target, that is not a benchmark-script issue; it is a ledger throughput release blocker.

## SOTA claim rule

Do not claim fastest/SOTA from any B6 artifact that has:

- empty `statusCodeDistribution`,
- non-empty `errorDistribution`,
- null latency percentiles,
- empty `rps` shell variable,
- or ledger drop log flood.
