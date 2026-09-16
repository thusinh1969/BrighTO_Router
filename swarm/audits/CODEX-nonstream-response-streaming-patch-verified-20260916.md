# PATCH READY — stream non-stream upstream body to client before EOF

Time: 2026-09-16 21:52 +07.  
Scope: current real worktree + temp prototype.  
Codex rule in this repo: patch/audit only; do not edit `src/` directly from Codex.

## Verdict

This patch fixes a real architecture blocker for production latency:

```bash
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
```

Current code buffers the full non-stream upstream response before returning it to the client. That disqualifies the router from a serious fastest/SOTA claim for large non-stream completions.

Apply order recommendation:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
```

The benchmark-truth patch still comes first. This patch should then be validated against truthful benchmark artifacts.

## Source problem fixed

Current real source:

```text
src/proxy/mod.rs:535   match response.bytes().await {
src/proxy/mod.rs:537       let acc = parse_usage_from_body(&body_bytes, format);
src/proxy/mod.rs:539       reporter.finish(...);
src/proxy/mod.rs:547       build_response(..., Body::from(body_bytes))
```

Current behavior:

```text
non-stream upstream response must reach EOF
then usage is parsed
then ledger/budget/metrics are finalized
then the client receives the response body
```

For large non-stream outputs or slow upstream bodies, this adds avoidable latency and memory pressure. It is not just a benchmark issue.

## Patch behavior

Patch file:

```text
audits/CODEX-apply-nonstream-response-streaming-20260916.patch
```

What it changes:

```text
src/proxy/mod.rs:
  - removes response.bytes().await from the normal non-stream path
  - uses response.bytes_stream() + bounded mpsc channel, same backpressure shape as stream=true
  - returns Body::from_stream(rx) immediately after upstream headers
  - accumulates response bytes only up to 1 MiB for final usage parse
  - if the cap is exceeded, clears the buffer and finalizes as estimated usage
  - finalizes ledger/budget/metrics exactly once on EOF/drop/error

tests/streaming_integration.rs:
  - adds delayed two-chunk non-stream backend
  - proves router returns response before upstream EOF
  - proves first body chunk arrives before delayed second chunk
  - proves usage is still parsed and ledger event is correct after EOF
```

No Redis. No new dependency. No DB work in request path. No rewrite of route/budget/ledger.

## Verified checks

Temp prototype path:

```text
/tmp/brigto_nonstream_streaming_KRvg95l9
```

Patch applies cleanly on current real worktree:

```text
$ git apply --check audits/CODEX-apply-nonstream-response-streaming-20260916.patch
PASS
```

Patch applies cleanly with benchmark-truth patch:

```text
$ git apply --check \
    audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch \
    audits/CODEX-apply-nonstream-response-streaming-20260916.patch
PASS
```

Patch applies cleanly with benchmark-truth + route-noalloc patches:

```text
$ git apply --check \
    audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch \
    audits/CODEX-apply-route-picker-noalloc-20260916.patch \
    audits/CODEX-apply-nonstream-response-streaming-20260916.patch
PASS
```

Prototype-only validation:

```text
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
cargo test --lib with real pgvector Postgres                   PASS: 45 passed
cargo test --test streaming_integration with real Postgres     PASS: 3 passed
```

Sequential all-three-patch validation path:

```text
/tmp/brigto_all_three_seq_TufHOxMr
```

Sequential order tested:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
git apply audits/CODEX-apply-route-picker-noalloc-20260916.patch
git apply audits/CODEX-apply-nonstream-response-streaming-20260916.patch
```

Checks after sequential apply:

```text
python3 -m py_compile scripts/bench_real.py                    PASS
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
cargo test -q route --lib                                      PASS: 8 passed
cargo test --test streaming_integration with real Postgres     PASS: 3 passed
```

## Acceptance after DeepSeek applies it

Run at minimum:

```bash
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --all-targets
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
cargo test --test streaming_integration
```

Then run truthful benchmark only after `CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch` is in the real tree:

```bash
DUR=5s WARM=1s RUNS=1 CONCS=50 REQUIRE_PASS=0 python3 scripts/bench_real.py
```

Keep the patch if it does not regress 1k/50k/200k and improves or preserves production response-start latency. This patch is mainly for production correctness/latency under large non-stream outputs; it may not solve the measured 200k upload-body miss by itself because that benchmark mock response is small.
## Update 2026-09-16 22:33 +07 — fresh target integration re-verified

A previous temp run used the shared Cargo target and reported only the old 2 integration tests. I reran with a fresh `CARGO_TARGET_DIR` in `/tmp/brigto_sequence_verify_eVjK6pf3` after applying the full current patch sequence. Result:

```text
running 3 tests
test stream_without_usage_records_estimate_not_zero ... ok
test stream_request_taps_usage_and_forwards_sse ... ok
test nonstream_response_starts_before_upstream_eof ... ok

test result: ok. 3 passed; 0 failed
```

This confirms the delayed two-chunk non-stream test from `CODEX-apply-nonstream-response-streaming-20260916.patch` is actually compiled and passed.

