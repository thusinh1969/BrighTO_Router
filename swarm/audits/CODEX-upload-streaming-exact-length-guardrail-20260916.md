# UPLOAD STREAMING GUARDRAIL — exact Content-Length is the hard requirement

Time: 2026-09-16 21:56 +07.  
Scope: current real worktree + local reqwest 0.13.5 source.  
Codex rule in this repo: audit only; do not edit `src/` directly from Codex.

## Verdict

Do not retry the previous upload-streaming idea with plain `reqwest::Body::wrap_stream`. It is the wrong primitive for this router unless the stream body exposes an exact byte `size_hint`.

The next upload-body optimization must choose one of these two paths:

```text
Path A: keep current full-buffer Bytes request body until timer-span profiling proves it is the top 200k cost.
Path B: implement streaming upload with exact Content-Length preserved via a custom HttpBody; benchmark it against the truthful harness before keeping it.
```

Do not add Redis. Do not touch Postgres. This is HTTP body mechanics only.

## Local reqwest evidence

Local crate:

```text
/home/steve/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reqwest-0.13.5
```

Async `Body::wrap_stream` exists:

```text
src/async_impl/body.rs:85   pub fn wrap_stream<S>(stream: S) -> Body
src/async_impl/body.rs:94   Body::stream(stream)
```

But `Body::stream` is crate-private and just boxes a `StreamBody`:

```text
src/async_impl/body.rs:97    pub(crate) fn stream<S>(stream: S) -> Body
src/async_impl/body.rs:106   let body = http_body_util::BodyExt::boxed(StreamBody::new(...))
src/async_impl/body.rs:111   inner: Inner::Streaming(body)
```

Content length for async request bodies comes from body size hint:

```text
src/async_impl/body.rs:161   pub(crate) fn content_length(&self) -> Option<u64>
src/async_impl/body.rs:163   Inner::Reusable(bytes)  => Some(bytes.len() as u64)
src/async_impl/body.rs:164   Inner::Streaming(body) => body.size_hint().exact()
src/async_impl/body.rs:263   fn size_hint(&self) -> http_body::SizeHint
src/async_impl/body.rs:266   Inner::Streaming(body) => body.size_hint()
```

Blocking reqwest has `Body::sized`, but async reqwest does not expose a public `Body::sized`:

```text
src/blocking/body.rs:80     pub fn sized<R: Read + Send + 'static>(reader: R, len: u64) -> Body
async_impl/body.rs:         no public Body::sized found
```

Conclusion: for async upload streaming, plain `Body::wrap_stream(stream)` is not enough unless the wrapped body reports exact byte length. The previous temp prototype used `wrap_stream(prefix_chunks.chain(rest))`; that path likely sent chunked/unknown-length upload and benchmarked worse.

## Negative benchmark already observed

From `CODEX-fast-upload-prototype-negative-result-20260916.md`:

```text
truth patch without naive upload streaming:
50k c=50 p50 +0.619ms p99 +1.048ms
200k c=50 p50 +1.651ms p99 +2.312ms

truth patch + naive Body::wrap_stream streaming:
50k c=50 p50 +1.452ms p99 +2.458ms
200k c=50 p50 +3.458ms p99 +4.824ms
```

This is enough to reject any patch that simply swaps `Bytes` for `wrap_stream` without exact length and fresh benchmark proof.

## If DeepSeek implements Path B

Minimal acceptable design:

```text
1. Add a direct dependency only if needed: http-body = "1" or http-body-util = "0.1".
2. Implement a tiny local body wrapper whose size_hint() returns SizeHint::with_exact(content_length).
3. Feed reqwest using reqwest::Body::wrap(custom_body), not plain wrap_stream.
4. Preserve retry semantics: once an upload stream is moved to a backend, no retry is possible after body consumption starts.
5. Keep full-buffer fallback for:
   - stream=true OpenAI request needing stream_options splice;
   - missing or suspicious Content-Length;
   - JSON prefix cannot prove top-level model and explicit stream=false;
   - small bodies where full-buffer Bytes is already faster/simpler;
   - any request where auth/model/route/budget cannot be resolved from a safe prefix.
```

Acceptance before keeping Path B:

```text
cargo fmt --all -- --check                                     PASS
CARGO_INCREMENTAL=0 cargo check --all-targets                  PASS
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings  PASS
targeted tests prove upstream receives exact Content-Length, not Transfer-Encoding: chunked
targeted tests prove nested/string model/stream fields do not activate the fast path
truthful benchmark shows 50k/200k p50/p99 improve versus the V2 baseline
truthful benchmark shows 1k does not regress
ledger_dropped_total remains 0 at target-rate B6
```

If these cannot be proven, keep the current `Bytes` upload path and optimize lower-risk costs first: route picker allocation, non-stream response buffering, and measured metric/header overhead.

## Current priority remains unchanged

Apply benchmark truth first:

```bash
git apply audits/CODEX-apply-remaining-benchmark-root-cause-fix-20260916.patch
```

Then measure. This upload-streaming guardrail only prevents another wrong 200k optimization attempt.
