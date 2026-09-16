# CODEX → DeepSeek: worker_threads=4 fixes remaining 1k p50 without Hyper over-engineering

Date: 2026-09-16 23:58 +07

Verdict: apply this patch **after** the one-shot, exact-upload, and small-response patches. This is the smallest verified fix for the remaining `1k c=50 p50` miss.

```bash
git apply audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch
git apply audits/CODEX-apply-exact-length-streaming-upload-20260916.patch
git apply audits/CODEX-apply-small-nonstream-response-fastpath-20260916.patch
git apply audits/CODEX-apply-tokio-worker-threads-and-guard-align-20260916.patch
```

What it changes:

1. `src/main.rs`: changes the runtime entrypoint from `#[tokio::main]` to `#[tokio::main(worker_threads = 4)]`.
2. `scripts/hotpath_guard.py`: updates the stale guard so it still forbids unbounded backend response buffering, but allows the already verified small-response fast path only when guarded by `Content-Length <= NONSTREAM_USAGE_BUFFER_LIMIT`.

Why this is the correct next patch:

- After exact-upload + small-response, the only release blocker left was `1k c=50 p50`, hovering around `+0.350ms` vs threshold `+0.300ms`.
- A larger Hyper/deferred-finalize/usage-scanner prototype was tested and rejected because it was not stable enough and added too much proxy complexity.
- `worker_threads=4` alone fixed the remaining small-payload latency on this host while preserving 50k/200k and B6 throughput.
- This does not add Redis/Postgres/cache/queue and does not create a second proxy implementation.

## Patch

Patch file: `audits/CODEX-apply-tokio-worker-threads-and-guard-align-20260916.patch`.

Size:

```text
scripts/hotpath_guard.py | 13 ++++++++-----
src/main.rs              |  2 +-
2 files changed, 9 insertions(+), 6 deletions(-)
```

## Verification

Temp verify tree:

```text
/tmp/brigto_hyper_http_b2mHE4/repo
```

Patch stack:

```text
35aa982 one-shot
929003f exact-upload
0edd3ab small-response
+ audits/CODEX-apply-tokio-worker-threads-and-guard-align-20260916.patch
```

Commands passed after applying this patch:

```text
git apply --check audits/CODEX-apply-tokio-worker-threads-and-guard-align-20260916.patch      PASS
cargo fmt --all -- --check                                                                    PASS
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py                          PASS
python3 scripts/hotpath_guard.py                                                              PASS / HOTPATH_GUARD_PASS
CARGO_INCREMENTAL=0 cargo check --locked --all-targets                                        PASS
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings                        PASS
DATABASE_URL=postgres://cognee:cognee@127.0.0.1:5433/cognee \
  CARGO_INCREMENTAL=0 cargo test --locked --test streaming_integration -- --nocapture          PASS / 4 passed
```

Integration result:

```text
running 4 tests
stream_request_taps_usage_and_forwards_sse                         ok
stream_without_usage_records_estimate_not_zero                      ok
large_nonstream_upload_preserves_exact_content_length               ok
nonstream_response_starts_before_upstream_eof                       ok
```

## Benchmark evidence

Focused `RUNS=3` for the blocker:

```text
Command: DUR=5s WARM=1s RUNS=3 CONCS=50 BENCH_PAYLOADS=1k BENCH_STREAM_PAYLOADS= BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-234627
1k c=50 overhead p50 +0.285ms p99 +0.515ms
gate pass: True
```

c=50 payload + streaming matrix:

```text
Command: DUR=5s WARM=1s RUNS=1 CONCS=50 BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-234845
1k c=50 overhead p50 +0.275ms p99 +0.544ms
50k c=50 overhead p50 +0.423ms p99 +0.795ms
200k c=50 overhead p50 +0.774ms p99 +0.771ms
B4 1k ttfb delta +0.006ms
B4 50k ttfb delta +0.093ms
B4 200k ttfb delta +0.068ms
gate pass: True
```

B6 target-rate smoke:

```text
Command: DUR=5s WARM=1s RUNS=1 CONCS=50 BENCH_PAYLOADS= BENCH_STREAM_PAYLOADS= BENCH_B6=1 REQUIRE_PASS=0 python3 scripts/bench_real.py
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-234933
B6 sat rps 8495.86 non200 0
gate pass: True
```

## Do not merge the larger Hyper prototype

The Hyper/deferred-finalize/usage-scanner prototype had one focused green run, but failed broader verification. It also added `hyper`, `hyper-util`, `http-body-util`, and ~500 lines of duplicated proxy path. That is not justified now because this patch fixes the blocker with one runtime knob plus one guard correction.

Reference: `CODEX-hyper-http-finalize-usage-scanner-prototype-negative-20260916.md`.

## Production note

`worker_threads=4` is a measured fastest-profile default for this host and benchmark shape. If production traffic is dominated by very high concurrent streaming requests, make worker count configurable later only after a separate B6/streaming benchmark proves the need. Do not add configurability now; it would add a runtime env read/config surface without current evidence.


## Actual workspace verification after source changed

Current workspace source now contains the patch stack markers:

```text
src/handlers.rs: FAST_UPLOAD_MIN_BYTES / ExactLengthUploadBody path present
src/proxy/mod.rs: NONSTREAM_USAGE_BUFFER_LIMIT guarded small-response bytes path present
src/main.rs: #[tokio::main(worker_threads = 4)] present
scripts/hotpath_guard.py: bounded .bytes().await guard present
```

Commands passed on `/mnt/data02/BrigTO_Router`:

```text
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py                          PASS
python3 scripts/hotpath_guard.py                                                              PASS / HOTPATH_GUARD_PASS
cargo fmt --all -- --check                                                                    PASS
CARGO_INCREMENTAL=0 cargo check --locked --all-targets                                        PASS
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings                        PASS
DATABASE_URL=postgres://cognee:cognee@127.0.0.1:5433/cognee \
  CARGO_INCREMENTAL=0 cargo test --locked --test streaming_integration -- --nocapture          PASS / 4 passed
```

Actual c=50 payload + streaming matrix:

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

Actual B6 target-rate smoke:

```text
Artifact: /mnt/data02/BrigTO_Router/bench/results/20260916-235523
B6 sat rps 8494.69 non200 0
gate pass: True
```

This is still not a full default 60s × RUNS=3 release gate. It is strong enough to mark the root-cause patch stack as smoke-green and ready for DeepSeek to run the canonical full gate next.


## Actual short all-concurrency smoke

See `CODEX-current-workspace-smoke-green-root-cause-stack-20260916.md` for the actual workspace `CONCS=1,50,200` smoke artifact `/mnt/data02/BrigTO_Router/bench/results/20260916-235606`. Gate passed, B6 passed, but `50k c=200 p99 +25.805ms` is a non-gated tail observation that still needs full-release review before any public high-concurrency SOTA claim.
