# CODEX → DeepSeek: exact upload + small-response patches verified; 200k fixed, 1k p50 still open

Date: 2026-09-16 23:16 +07

Verdict: apply the existing one-shot patch first, then these two follow-up patches in order. They are measured improvements and they do not add Redis/Postgres/cache/queues to the hot path.

```bash
git apply audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch
git apply audits/CODEX-apply-exact-length-streaming-upload-20260916.patch
git apply audits/CODEX-apply-small-nonstream-response-fastpath-20260916.patch
```

Do **not** claim full SOTA gate green after these patches. They fix the payload-scaling/root-tail problems, but `1k c=50` p50 still misses the current B1 threshold slightly on this host.

## Patch 1: exact-length streaming upload

File: `audits/CODEX-apply-exact-length-streaming-upload-20260916.patch`.

What it changes:

- Adds a bounded top-level request-head scanner in `src/handlers.rs`.
- Keeps auth before body work.
- For large bodies with valid `Content-Length`, top-level `model`, and explicit top-level `stream:false`, it routes after reading only the prefix and passes `prefix + remaining_body_stream` to proxy.
- Preserves exact upstream `Content-Length` with a custom `http_body::Body` whose `size_hint()` returns exact remaining bytes.
- Avoids the previously rejected naive `reqwest::Body::wrap_stream` path.
- Keeps full-buffer + serde fallback for missing/suspicious length, prefix miss, stream mutation, small bodies, and routes with fallback/multiple backend retry semantics.

This is the correct root-cause fix for the measured 200k serialization problem: current architecture reads 800KB into router, parses it, then uploads 800KB again. This patch overlaps client ingress with backend upload for the safe pass-through case.

## Patch 2: small non-stream response fast path

File: `audits/CODEX-apply-small-nonstream-response-fastpath-20260916.patch`.

What it changes:

- For non-stream backend responses with `Content-Length <= 1MiB`, it uses `response.bytes().await`, parses usage, and returns `Body::from(bytes)`.
- For missing or large `Content-Length`, it keeps the one-shot streaming response path, so delayed/large non-stream output still starts before upstream EOF.
- Purpose: remove per-request spawn + mpsc from tiny JSON benchmark responses, which was causing the observed 40ms tail on `1k c=50`.

## Verification: patch application and compile gates

Fresh verify tree:

```text
/tmp/brigto_exact_upload_verify_biajSt/repo
```

Commands passed after applying one-shot + exact-upload + small-response patches:

```text
git apply --check audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch                  PASS
git apply --check audits/CODEX-apply-exact-length-streaming-upload-20260916.patch             PASS
git apply --check audits/CODEX-apply-small-nonstream-response-fastpath-20260916.patch         PASS
cargo fmt --all -- --check                                                                    PASS
python3 -m py_compile scripts/bench_real.py scripts/hotpath_guard.py                          PASS
python3 scripts/hotpath_guard.py                                                              PASS / HOTPATH_GUARD_PASS
CARGO_INCREMENTAL=0 cargo check --locked --all-targets                                        PASS
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings                        PASS
```

Postgres integration test with `pgvector/pgvector:pg16` also passed:

```text
CARGO_INCREMENTAL=0 cargo test --locked --test streaming_integration -- --nocapture
running 4 tests
stream_request_taps_usage_and_forwards_sse                         ok
large_nonstream_upload_preserves_exact_content_length               ok
stream_without_usage_records_estimate_not_zero                      ok
nonstream_response_starts_before_upstream_eof                       ok
```

The new integration test proves the large non-stream upload fast path sends exact upstream `Content-Length`, no `Transfer-Encoding: chunked`, and preserves body length.

## Benchmark evidence

One-shot baseline, before exact-upload patch:

```text
Artifact: /tmp/brigto_one_shot_baseline_compare/repo/bench/results/20260916-230154
1k c=50 overhead p50 +0.377ms p99 +43.769ms
```

Exact-upload patch, focused 200k:

```text
Artifact: /tmp/brigto_exact_upload_1Yp5lP/repo/bench/results/20260916-225446
200k c=50 overhead p50 +0.854ms p99 +1.131ms
gate pass: True for focused 200k/B1/B2 without B6
```

Exact-upload + small-response patch, full non-B6 matrix:

```text
Artifact: /tmp/brigto_exact_upload_1Yp5lP/repo/bench/results/20260916-231113
1k c=50 overhead p50 +0.350ms p99 +0.588ms
50k c=50 overhead p50 +0.526ms p99 +0.938ms
200k c=50 overhead p50 +0.800ms p99 +1.522ms
B4 1k ttfb delta +0.022ms
B4 50k ttfb delta +0.029ms
B4 200k ttfb delta -0.010ms
B6 skipped
gate pass: False
```

Interpretation:

- 200k B1/B2 is fixed by exact-length streaming upload.
- 50k B1/B2 is green.
- 1k p99 tail is fixed by small non-stream response fast path.
- Full gate remains red because 1k p50 is `+0.350ms` and threshold is `+0.300ms`.

## Negative attempts already measured

Do not chase these as next fix without new evidence:

```text
tcp_nodelay(true): /tmp/brigto_exact_upload_1Yp5lP/repo/bench/results/20260916-230352
1k c=50 overhead p50 +0.419ms p99 +43.677ms
verdict: not the 1k tail fix

fast path for every Content-Length body, including 1k: /tmp/brigto_exact_upload_1Yp5lP/repo/bench/results/20260916-230845 and 20260916-230905
1k c=50 overhead p50 +0.316ms/+0.362ms p99 +0.562ms/+0.599ms
verdict: not reliable enough; keep 64KiB min for first production patch

defer small-response finalize into tokio::spawn: /tmp/brigto_exact_upload_1Yp5lP/repo/bench/results/20260916-231447
1k c=50 overhead p50 +0.332ms p99 +0.529ms
verdict: not enough, and it delays concurrency/lease release by scheduler timing
```

## Next root-cause work: 1k p50 only

After applying the three patches above, do **not** work on Redis/Postgres/cache/queue, and do **not** rollback exact upload or small-response fast path. The remaining miss is a small-request p50 budget problem, not a large-prompt architecture problem.

Next measurement should isolate these spans on `1k c=50` with microsecond timers:

```text
handler.auth_hash_and_snapshot
handler.small_body_collect
handler.serde_head_parse
handler.model_allowed_and_route_lookup
budget.reserve_and_concurrency
proxy.build_reqwest_request
proxy.execute_to_headers
proxy.small_response_bytes
proxy.parse_usage
finish.ledger_try_record
finish.metrics_emit
```

Likely candidates to inspect first, because they still run before the client sees the full small response:

- metrics label allocation in `request_total`, `tokens_total`, `observe_ttfb`, `observe_overhead`;
- `BudgetReservation` heap allocation / `Vec<ReservedScope>` for fixed small scope counts;
- request-id string formatting;
- reqwest request build/header cloning.

Acceptance before claiming SOTA:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 BENCH_B6=0 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Required minimum from artifact:

```text
1k c=50 p50 <=0.300ms and p99 <=0.800ms
50k c=50 p50 <=0.600ms and p99 <=1.500ms
200k c=50 p50 <=1.000ms and p99 <=2.000ms
ledger_dropped_total == 0
```

Then run the full gate with B6 enabled before release claim.
