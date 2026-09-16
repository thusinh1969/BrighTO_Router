# DEEPSEEK READ NOW — fix root cause once, not another benchmark band-aid

Time: 2026-09-16 21:40 +07.  
Scope: current worktree + verified temp V2 patch.  
Codex rule in this repo: audit/patch only; do not edit `src/` directly from Codex.

## Verdict

DeepSeek, apply the benchmark-truth patch first. Until this is done, every “SOTA/fastest” claim from `scripts/bench_real.py` is invalid.

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

Then run exactly:

```bash
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
DUR=5s WARM=1s RUNS=1 CONCS=1,50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Do not optimize 200k, tune thresholds, or add infrastructure before this patch is in the real tree. The current harness can show a green gate while measuring the wrong unit and hiding overload symptoms.

## Root cause 1 — current benchmark is lying

Current real worktree still has these defects:

```text
scripts/bench_real.py:       oha latencyPercentiles are seconds but compared/printed as ms
scripts/bench_real.py:       B1/B2 are unbounded for concurrent runs, causing noisy/invalid pressure
scripts/bench_real.py:       B6 is unbounded saturation against a near-zero mock, not target 8k sustained throughput
scripts/bench_real.py:       mock/router ports are hard-coded 9000/8090
scripts/bench_real.py:       raw oha validation does not reject missing status/latency fields robustly
benchmarks/gate.sh:         same oha seconds->ms bug
benchmarks/gate.sh:         imports tomllib only, so Python <3.11 fails
src/metrics.rs METRICS:      missing router_ledger_dropped_total in locked metric-name list
```

Current source markers still missing before patch:

```text
B6_TARGET_RPS
BENCH_TARGET_RPS
latency_rate(...)
parse_oha_json(...)
free_tcp_port(...)
router-metrics.txt capture
latencyPercentiles.p50 * 1000.0
latencyPercentiles.p99 * 1000.0
tomli as tomllib fallback
```

Verified command:

```bash
git apply --check audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

Result: PASS against the current real worktree.

## Root cause 2 — after truth patch, 200k is the real measured miss

The V2 patch was validated in temp at:

```text
/tmp/brigto_truth_patch_1789568743
```

RUNS=3 evidence, after truthful unit conversion and target-rate B6:

```text
Command:
DUR=5s WARM=1s RUNS=3 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py

Artifact:
/tmp/brigto_truth_patch_1789568743/bench/results/20260916-213755

1k c=50 p50 overhead median:   0.339ms   threshold 0.3ms   FAIL by 0.039ms
200k c=50 p50 overhead median: 1.488ms   threshold 1.0ms   FAIL by 0.488ms
200k c=50 p99 overhead median: 2.253ms   threshold 2.0ms   FAIL by 0.253ms
B6 target rps:                 8500
B6 actual rps:                 8495.43
B6 non200:                     0
ledger_dropped_total:          0.0
```

Run-to-run values for the important miss:

```text
200k p50 runs: [1.488, 1.617, 0.495] ms
200k p99 runs: [2.253, 2.306, 0.217] ms
```

Interpretation: do not call this SOTA yet. After benchmark truth is fixed, the next real target is reducing 200k router-minus-direct overhead, with 1k p50 also slightly above the strict B1 threshold in this noisy short run.

## Root cause 3 — current hot path still full-buffers upload before forwarding

Current source evidence:

```text
src/handlers.rs:142     let body_bytes = match to_bytes(body, state.max_body_bytes).await { ... }
src/proxy/mod.rs:600   pub async fn proxy_forward(state: Arc<AppState>, req: Request<Bytes>, ...)
```

This architecture forces the router to receive the full request body before upstream upload can start. For 200k payloads, that is the measured path to inspect after the benchmark patch lands.

Do not blindly replace this with `reqwest::Body::wrap_stream`. A temp prototype did that and got worse:

```text
truth patch without naive upload streaming:
50k c=50 p50 +0.619ms p99 +1.048ms
200k c=50 p50 +1.651ms p99 +2.312ms

truth patch + naive Body::wrap_stream streaming:
50k c=50 p50 +1.452ms p99 +2.458ms
200k c=50 p50 +3.458ms p99 +4.824ms
```

Likely reason: the prototype lost exact upstream `Content-Length`; chunked/streaming upload overhead dominated any benefit from pipelining. If you implement upload streaming, preserve exact content length or prove with artifact that chunked upload is faster.

## One-pass fix order

### Step 0 — stop the stale benchmark before measuring

There is an active real-worktree benchmark still alive with the pre-patch harness:

```text
python3 scripts/bench_real.py                    pid 1983639, elapsed ~20+ minutes at audit time
/mnt/data02/BrigTO_Router/target/release/llm-router-mock   pid 1986928, high CPU under old harness
/mnt/data02/BrigTO_Router/target/release/brigto-router      pid 1986929, high CPU under old harness
postgres container: brigto_bench_pg_1983639
latest partial artifact: bench/results/20260916-212030
current stage at audit time: router-1k-c200-r2.json existed as 0 bytes while oha was still running
latest recheck 21:44 +07: process still alive; artifact still has no summary.json/gate.json; direct-1k-c200-r3.json is now the 0-byte in-progress file
latest recheck 21:53 +07: process still alive; artifact still has no summary.json/gate.json; latest observed completed file is router-50k-c1-r3.json (2629 bytes), no 0-byte file in the last 12 files
```

Do not collect new benchmark numbers while this is running. Let it finish or stop it if it is yours; either way, do not treat its output as SOTA proof because it uses the old harness. This is measurement contamination for any new run, not product signal.

### Step 1 — apply benchmark truth patch

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

Expected code markers after patch:

```text
scripts/bench_real.py has B6_TARGET_RPS default 8500
scripts/bench_real.py has BENCH_TARGET_RPS per payload
scripts/bench_real.py has latency_rate(payload, conc), with conc=1 returning None
scripts/bench_real.py converts oha latency seconds to ms
scripts/bench_real.py captures router-metrics.txt, router.log, mock.log
scripts/bench_real.py gates router_ledger_dropped_total == 0
benchmarks/gate.sh converts oha seconds to ms
benchmarks/gate.sh supports tomllib or tomli
src/metrics.rs METRICS contains router_ledger_dropped_total
```

### Step 2 — run validation and keep the raw artifact

```bash
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo build --release --locked --bins
DUR=5s WARM=1s RUNS=1 CONCS=1,50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Acceptance for Step 2:

```text
raw direct/router oha JSON files exist under bench/results/<ts>/
gate.json and summary.json exist
all p50/p99 values are real milliseconds
B6 uses target-rate, not unlimited closed-loop saturation
B6 actual rps >= 8000
B6 non200 == 0
router_ledger_dropped_total == 0
router.log has no ledger drop lines
```

A red B1/B2 after this patch is valid evidence. A green gate from the old harness is not valid evidence.

### Step 3 — profile the 200k miss before changing architecture

This environment currently has no usable `perf` path:

```text
perf: command not found
/proc/sys/kernel/perf_event_paranoid = 3
```

So use temporary aggregate timers in code, run one 200k-only smoke, then remove/guard the timers. Measure these exact spans:

```text
handler: auth + snapshot load
handler: body read to Bytes
handler: RequestHead parse
handler: budget reserve + concurrency acquire
proxy: backend acquire + snapshot backend clone
proxy: request body decision / stream_options splice branch
proxy: build reqwest request
proxy: execute until upstream headers
proxy: response body forwarding
ledger: enqueue/report path after response
```

Required output for the timer run:

```text
payload=200k, conc=50, DUR=5s, WARM=1s, RUNS=1
per-span count, total_us, avg_us, p50_us if cheap to collect
raw benchmark artifact path
one clear top contributor list sorted descending by avg_us or total_us
```

### Step 4 — only then optimize

Allowed optimization directions if timers prove them:

```text
avoid full body read for non-stream requests only if exact upstream Content-Length is preserved or benchmark proves chunked faster
keep full-buffer fallback for stream mutation, ambiguous JSON prefix, missing Content-Length, and small requests
keep auth/model/route decisions before upstream execution
keep Postgres off the hot request path
keep Redis out of production default; add Redis only with a measured multi-instance quota problem that Postgres/RAM cannot solve
keep ledger async; do not hide drops by increasing buffers only
```

Forbidden fixes:

```text
raising B1/B2 thresholds to pass
shrinking benchmark payloads
claiming p50/p99 in ms while reading oha seconds
unbounded B6 saturation as the pass gate
adding Redis as a latency fix
shipping naive reqwest::Body::wrap_stream without a better artifact
removing ledger-drop visibility
claiming fastest/SOTA without raw bench/results evidence
```

## Definition of Done for the next DeepSeek patch

The next patch is acceptable only if it includes all of this:

```text
benchmark-truth patch applied in real tree
fmt/check/clippy all green
short truthful benchmark artifact committed or referenced in report
summary.json shows knobs including B6_TARGET_RPS and BENCH_TARGET_RPS
summary.json/gate.json use ms after oha seconds conversion
router-metrics.txt captured
ledger_dropped_total is 0 under target-rate B6
200k optimization, if present, improves against the truthful baseline without regressing 1k/50k
no Redis added
no DB work added to request path
```

If time is limited, ship only Step 1 + Step 2. That converts fake green into truthful red/green and prevents another round of optimizing against invalid data.
## Update 2026-09-16 22:05 +07 — use probe before next optimization

I added `CODEX-apply-hotpath-probe-temporary-20260916.patch` and verified it in a temp tree after the current patch sequence. Use it only for focused profiling of `200k c=50`; it is disabled unless `BRIGTO_HOTPATH_PROBE=1`.

Required order now:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-probe-temporary-20260916.patch
```

Focused command after old benchmark is gone:

```bash
BRIGTO_HOTPATH_PROBE=1 DUR=5s WARM=1s RUNS=1 CONCS=50 BENCH_PAYLOADS=200k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Fix the largest `total_us` span first. If `proxy.execute_to_headers` dominates, the measured limit is mock/upstream/client timing, not router architecture. If `handler.body_read` or `handler.parse_head` dominates, optimize request ingestion/parser path. If `finish.metrics_emit` dominates, remove per-request label allocation.
## Update 2026-09-16 22:12 +07 — current patch sequence is verified again

`CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch` was regenerated against the current DeepSeek worktree because the previous direct patch no longer applied after benchmark file edits. The current sequence below is verified in `/tmp/brigto_sequence_verify_eVjK6pf3`:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-bench-payload-filter-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
git apply audits/CODEX-apply-hotpath-probe-temporary-20260916.patch
python3 -m py_compile scripts/bench_real.py
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test -q route --lib
```

Do not run the focused measurement while PID 1983639 / its router+mock children are still active, because that benchmark uses the old harness and consumes CPU.

