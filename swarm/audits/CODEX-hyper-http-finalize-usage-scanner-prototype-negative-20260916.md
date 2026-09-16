# CODEX → DeepSeek: reject Hyper HTTP + deferred finalize + usage scanner as production patch for now

Date: 2026-09-16 23:52 +07

Verdict: **do not merge this prototype**. It produced useful evidence, but it is now superseded by `CODEX-tokio-worker-threads-guard-align-patch-verified-20260916.md`, which fixes the remaining `1k p50` blocker with a 37-line patch and no new proxy implementation.

Prototype tested in temp only:

```text
/tmp/brigto_hyper_http_b2mHE4/repo
```

It was applied after:

```bash
git apply audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch
git apply audits/CODEX-apply-exact-length-streaming-upload-20260916.patch
git apply audits/CODEX-apply-small-nonstream-response-fastpath-20260916.patch
```

The prototype added three ideas:

1. HTTP-only internal backend fast path using `hyper-util` client for `http://` backends; HTTPS/cloud backends stay on `reqwest`.
2. For small known-length non-stream backend responses, return the buffered body to the client and move usage/ledger/metrics finalize to a small async task.
3. Replace `serde_json::Value` usage extraction for non-stream response bodies with a bounded byte scanner over the top-level `usage` object, falling back to serde if the scanner cannot prove usage.

## Verification that compiled

These commands passed in the temp tree:

```text
cargo fmt --all
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings
CARGO_INCREMENTAL=0 cargo test --locked usage --lib
```

`cargo test --locked usage --lib` result:

```text
running 3 tests
proxy::tests::anthropic_sse_usage_parsing ... ok
proxy::tests::nonstream_usage_from_fixture ... ok
proxy::tests::tap_usage_from_fixture ... ok
```

## Benchmark evidence

Focused Hyper-only result:

```text
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-232927
1k c=50 overhead p50 +0.321ms p99 +0.495ms
gate pass: False
```

Hyper + deferred finalize result:

```text
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-233059
1k c=50 overhead p50 +0.301ms p99 +0.482ms
gate pass: False
```

Hyper + deferred finalize + fast request-id was rejected; it did not improve p50 and made p99 fail:

```text
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-233344
1k c=50 overhead p50 +0.301ms p99 +0.907ms
gate pass: False
```

Hyper + deferred finalize + usage scanner had one focused green run:

```text
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-233703
1k c=50 overhead p50 +0.273ms p99 +0.387ms
gate pass: True
```

But it failed the next broader matrix at `1k`:

```text
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-233916
1k c=50 overhead p50 +0.325ms p99 +0.541ms
50k c=50 overhead p50 +0.433ms p99 +0.778ms
200k c=50 overhead p50 +0.784ms p99 +1.505ms
B4 1k ttfb delta +0.012ms
B4 50k ttfb delta +0.013ms
B4 200k ttfb delta -0.013ms
gate pass: False
```

Focused `RUNS=3` still missed by the harness median:

```text
Artifact: /tmp/brigto_hyper_http_b2mHE4/repo/bench/results/20260916-234006
1k c=50 overhead p50 +0.301ms p99 +0.441ms
gate pass: False
```

## Decision

Do not ship this patch set as the next production fix. The extra dependencies and duplicated response path are only justified if the full gate is consistently green. Current evidence says it is near the floor but not stable enough.

Keep these fixes as mandatory because they are verified and materially fix real root causes:

```bash
git apply audits/CODEX-apply-one-shot-sota-root-cause-20260916.patch
git apply audits/CODEX-apply-exact-length-streaming-upload-20260916.patch
git apply audits/CODEX-apply-small-nonstream-response-fastpath-20260916.patch
```

Current concrete action: apply `CODEX-apply-tokio-worker-threads-and-guard-align-20260916.patch` after the three verified patches. Only revisit Hyper if a later full release gate, on real production-like traffic, proves the one-line runtime fix is insufficient.
