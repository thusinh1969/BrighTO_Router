# CODEX → DeepSeek: current workspace smoke-green after root-cause patch stack

Date: 2026-09-17 01:18 +07

Verdict: current source is now smoke-green for the measured SOTA hot path. Do not add Redis/Postgres/cache/Hyper. The next action is the canonical full release gate, not more architecture churn.

Current source markers checked:

```text
src/handlers.rs: FAST_UPLOAD_MIN_BYTES present; exact-length streaming upload path present
src/proxy/mod.rs: ExactLengthUploadBody present; NONSTREAM_USAGE_BUFFER_LIMIT guarded small-response path present
src/main.rs: #[tokio::main(worker_threads = 4)] present
scripts/hotpath_guard.py: bounded .bytes().await guard present
```

Commands passed on `/mnt/data02/BrigTO_Router`:

```text
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py
python3 scripts/hotpath_guard.py                                      # HOTPATH_GUARD_PASS
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings
DATABASE_URL=postgres://cognee:cognee@127.0.0.1:5433/cognee \
  CARGO_INCREMENTAL=0 cargo test --locked --test streaming_integration -- --nocapture
```

Postgres integration result:

```text
running 4 tests
stream_without_usage_records_estimate_not_zero                      ok
stream_request_taps_usage_and_forwards_sse                         ok
large_nonstream_upload_preserves_exact_content_length               ok
nonstream_response_starts_before_upstream_eof                       ok
```

Actual smoke benchmark evidence:

```text
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260916-235254
1k c=50 overhead p50 +0.255ms p99 +0.569ms
50k c=50 overhead p50 +0.502ms p99 +0.715ms
200k c=50 overhead p50 +0.798ms p99 +1.295ms
B4 1k ttfb delta +0.016ms
B4 50k ttfb delta -0.053ms
B4 200k ttfb delta -0.012ms
gate pass: True
```

B6 smoke:

```text
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260916-235523
B6 sat rps 8494.69 non200 0
gate pass: True
```

What remains before any public “fastest/SOTA” claim:

```bash
# Run canonical full release gate, not another custom ad-hoc benchmark.
python3 scripts/bench_real.py
```

Required release proof: default `DUR=60s`, `WARM=15s`, `RUNS=3`, `CONCS=1,50,200`, `BENCH_B6=1`, all artifacts retained under `bench/results/<run-id>/`, and `gate.json.pass == true` with no ledger drops.


Additional short all-concurrency smoke:

```text
Command: DUR=5s WARM=1s RUNS=1 CONCS=1,50,200 BENCH_B6=1 REQUIRE_PASS=0 python3 scripts/bench_real.py
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260916-235606
1k c=1 overhead p50 +0.250ms p99 +0.332ms
1k c=50 overhead p50 +0.300ms p99 +0.483ms
1k c=200 overhead p50 +0.252ms p99 +2.438ms
50k c=1 overhead p50 +0.320ms p99 +0.682ms
50k c=50 overhead p50 +0.526ms p99 +0.598ms
50k c=200 overhead p50 +0.465ms p99 +25.805ms
200k c=1 overhead p50 +0.657ms p99 +0.537ms
200k c=50 overhead p50 +0.833ms p99 +0.850ms
200k c=200 overhead p50 +0.848ms p99 -1.897ms
B4 1k ttfb delta -0.042ms
B4 50k ttfb delta -0.039ms
B4 200k ttfb delta -0.006ms
B6 sat rps 8495.05 non200 0
gate pass: True
```

Caveat: the current gate only enforces B1/B2 at c=50. The `50k c=200 p99 +25.805ms` value is not gate-blocking, but it must not be ignored in a public SOTA claim. Run the default full gate and inspect c=200 raw tails before claiming high-concurrency p99 leadership.


## Canonical full release gate result

The canonical full release gate completed after the smoke checks:

```text
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260917-000839
Command: python3 scripts/bench_real.py
DUR=60s WARM=15s RUNS=3 CONCS=1,50,200 BENCH_B6=1
gate pass: True
```

Full details are now in `CODEX-canonical-full-release-gate-green-20260917.md`.
