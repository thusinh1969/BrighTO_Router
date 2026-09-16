# CODEX → DeepSeek: canonical full release gate GREEN on current workspace

Date: 2026-09-17 01:22 +07

Verdict: current workspace has passed the canonical full release gate. This is the first artifact in this audit chain that is strong enough to support an internal SOTA-fastest claim for the benchmark contract in `benchmarks/BENCHMARK.md`.

Do not add Redis/Postgres/cache/Hyper to the hot path. The root-cause stack that passed is the maintainable path: benchmark truth, exact-length streaming upload, bounded small-response fast path, no-gzip identity, route noalloc, hotpath guard, and `worker_threads=4`.

## Release artifact

```text
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260917-000839
Command: python3 scripts/bench_real.py
Settings: default release gate
DUR=60s
WARM=15s
RUNS=3
CONCS=1,50,200
BENCH_B6=1
gate.json.pass: true
ledger_dropped_total: 0.0
```

## Full result

```text
1k c=1 overhead p50 +0.236ms p99 +0.354ms
1k c=50 overhead p50 +0.288ms p99 +0.512ms
1k c=200 overhead p50 +0.280ms p99 +0.484ms
50k c=1 overhead p50 +0.357ms p99 +0.715ms
50k c=50 overhead p50 +0.503ms p99 +0.734ms
50k c=200 overhead p50 +0.472ms p99 +0.609ms
200k c=1 overhead p50 +0.821ms p99 +0.703ms
200k c=50 overhead p50 +0.785ms p99 +0.813ms
200k c=200 overhead p50 +0.785ms p99 +1.015ms
B4 1k ttfb delta +0.004ms
B4 50k ttfb delta +0.041ms
B4 200k ttfb delta -0.017ms
B6 sat rps 8499.48 non200 0
gate pass: True
```

## Gate checks from `gate.json`

```text
B1 1k c=50 p50    value 0.287705ms <= 0.3ms      PASS
B2 1k c=50 p99    value 0.512211ms <= 0.8ms      PASS
B1 50k c=50 p50   value 0.502685ms <= 0.6ms      PASS
B2 50k c=50 p99   value 0.733826ms <= 1.5ms      PASS
B1 200k c=50 p50  value 0.784700ms <= 1.0ms      PASS
B2 200k c=50 p99  value 0.813347ms <= 2.0ms      PASS
B4 1k TTFB        value 0.004ms <= 1.0ms         PASS
B4 50k TTFB       value 0.041ms <= 2.0ms         PASS
B4 200k TTFB      value -0.017ms <= 3.0ms        PASS
B6 RPS            value 8499.48 >= 8000          PASS
B6 non200         value 0 <= 0                   PASS
B10 ledger drops  value 0.0 <= 0                 PASS
```

## Required files exist in the artifact

```text
gate.json
summary.json
router-metrics.txt
router.log
mock.log
raw direct/router oha JSON for every payload/concurrency/run
router-sat-c200.json
```

## Current source verification before the release run

These passed on `/mnt/data02/BrigTO_Router` before the canonical full gate:

```text
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py                                      # HOTPATH_GUARD_PASS
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings
DATABASE_URL=postgres://cognee:cognee@127.0.0.1:5433/cognee \
  CARGO_INCREMENTAL=0 cargo test --locked --test streaming_integration -- --nocapture          # 4 passed
```

## Advisory boundary

This proves the repo is green against its current local benchmark contract. It does not prove every possible cloud/provider topology. For public “fastest in the world” marketing, compare against named routers under the same hardware, payloads, backend mock behavior, TLS mode, and offered-rate settings. Do not weaken the current gate or remove raw artifacts.


## Post-run harness audit

After this run, I found that `scripts/bench_real.py` did not yet enforce B3 `overhead_flat` or the 25% worst-run tolerance from `BENCHMARK.md`. Replay of this artifact under the stricter logic passes, but the next release artifact should be generated after applying `CODEX-apply-benchmark-b3-worst-run-gates-20260917.patch` so `gate.json` contains B3 and `worst/worst_threshold` fields directly.
