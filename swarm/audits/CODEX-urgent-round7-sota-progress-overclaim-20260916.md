# URGENT — Round 7 SOTA progress is overclaimed; current evidence is not enough

Time: 2026-09-16 22:xx ICT  
Scope: current worktree + `swarm/out/PROGRESS.md`. Codex did not edit `src/`.

## Verdict

`swarm/out/PROGRESS.md` Round 7 must be treated as **overclaim**, not release evidence.

Round 7 says:

```text
REAL SOTA benchmark artifact ...
1k:   p50 +0.000ms  p99 +0.000ms
50k:  p50 +0.000ms  p99 +0.000ms
200k: p50 +0.000ms  p99 +0.001ms
=> Non-stream hot path overhead ~0-1 microsecond o moi payload, RAT duoi target 2ms.
```

That is useful smoke data, but it does **not** prove SOTA readiness because current source/harness still has verified blockers.

## Contradictions in current evidence

### 1. Current source still has `stream_options` false-positive bug

Source:

```text
src/proxy/mod.rs:46 fn contains_stream_options(body: &[u8]) -> bool {
src/proxy/mod.rs:47     memchr::memmem::find(body, b"\"stream_options\"").is_some()
src/proxy/mod.rs:647 && !contains_stream_options(&body)
```

Runtime smoke:

```text
CASE normal_missing_root status 200 top_has True include_true True
CASE string_false_positive status 200 top_has False include_true False
CASE nested_false_positive status 200 top_has False include_true False
RESULT FAIL
```

So Round 7 cannot claim `stream_options` root-level detection is fixed in current source.

### 2. Current benchmark B6 gate measures the wrong concurrency

Source in both benchmark gate copies:

```bash
CONC=200 read -r _ _ rps rn < <(run_oha "$ROUTER_URL" "1k.json" "$R/router-sat.json")
```

Shell proof:

```text
x=50 outer=50
```

So B6 throughput cannot be trusted until `run_oha` accepts explicit concurrency or `CONC` is set before process substitution.

### 3. Existing artifact is not the required SOTA matrix

Current artifact found:

```text
bench/results/20260916-200856/summary.json
```

It covers only:

- non-stream
- payloads 1k/50k/200k
- concurrency 50
- one run summary

It does not cover the benchmark spec’s required matrix:

- concurrency 1 / 50 / 200
- stream and non-stream
- warm-up 15s + measured 60s
- 3 runs + median
- B4 streaming TTFB delta
- B5 chunk gap
- B6 saturation throughput after fixing concurrency bug
- B7/B8 memory/stability
- B10/B11 ledger lag/Postgres outage

## Required correction

Update `swarm/out/PROGRESS.md` or future status to say:

```text
Round 7 produced a useful non-stream conc=50 smoke artifact, not a full SOTA gate. Current blockers before SOTA claim:
1. Fix stream_options top-level detection false positive.
2. Fix benchmark B6 concurrency shell bug.
3. Re-run benchmark matrix after fixes and publish fresh gate.json.
```

Do not delete the smoke result; just label it correctly.

## Exact next patches

1. Apply `CODEX-p1-stream-options-root-detection-false-positive-20260916.md`.
2. Apply `CODEX-benchmark-gate-b6-concurrency-bug-20260916.md`.
3. Re-run gates after both are fixed:

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
python3 /tmp/brigto_stream_options_false_positive.py
python3 /tmp/brigto_invalid_json_smoke.py
python3 /tmp/brigto_p0_smoke.py
/tmp/brigto-cargo-tools/bin/cargo-audit audit --file Cargo.lock --json
```

4. Then run real benchmark gate and attach the fresh artifact. The existing `bench/results/20260916-200856/summary.json` is not enough for a fastest-in-world claim.
