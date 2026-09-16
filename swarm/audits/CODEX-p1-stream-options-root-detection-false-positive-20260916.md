# P1 — `stream_options` detection is false-positive; use top-level serde result, not body-wide memmem

Time: 2026-09-16 21:xx ICT  
Scope: current worktree. Codex did not edit `src/`.

## Verdict

Current OpenAI streaming injection is **not production-correct** for all valid requests.

Root cause: `src/proxy/mod.rs::contains_stream_options()` scans the entire raw request body for bytes `"stream_options"`. That detects occurrences inside message content or nested objects and then skips router injection, even though OpenAI/llama-server need top-level `stream_options.include_usage=true` to return final usage.

This is not a theoretical code-style issue. Runtime smoke captured the upstream body and proved the router skips injection incorrectly.

## Runtime proof

Command used:

```bash
cargo build --release --locked
python3 /tmp/brigto_stream_options_false_positive.py
```

Captured upstream bodies:

```text
CASE normal_missing_root status 200 top_has True include_true True
raw b'{"model":"x","stream":true,"messages":[{"role":"user","content":"hello"}],"stream_options":{"include_usage":true}}'

CASE string_false_positive status 200 top_has False include_true False
raw b'{"model":"x","stream":true,"messages":[{"role":"user","content":"stream_options"}]}'

CASE nested_false_positive status 200 top_has False include_true False
raw b'{"model":"x","stream":true,"messages":[{"role":"user","content":"hello","stream_options":{"include_usage":false}}]}'

RESULT FAIL
```

Expected behavior: all three cases have `stream=true` and no top-level `stream_options`, so all three forwarded bodies should contain top-level:

```json
"stream_options":{"include_usage":true}
```

Actual behavior: only the plain case is injected. A user prompt containing the literal text `stream_options` disables injection.

## Source root cause

Current code:

```rust
fn contains_stream_options(body: &[u8]) -> bool {
    memchr::memmem::find(body, b"\"stream_options\"").is_some()
}

let request_body = if backend.format == BackendFormat::OpenAi
    && stream_request
    && !contains_stream_options(&body)
{
    splice_include_usage(&body)
        .map(Bytes::from)
        .unwrap_or_else(|| body.clone())
} else {
    body.clone()
};
```

The body-wide substring check cannot distinguish these cases:

```json
{"stream_options":{"include_usage":false}}                   // top-level, do not inject
{"messages":[{"content":"stream_options"}]}                 // nested/string, must inject
{"messages":[{"stream_options":{"include_usage":false}}]}   // nested object, must inject
```

## Exact patch contract

Do **not** add another full JSON parse in proxy. The handler already performs a top-level serde parse into `RequestHead<'_>` before calling proxy. Extend that parse to capture top-level presence and pass one bool into `ProxyContext`.

### 1. In `src/handlers.rs`, borrow raw top-level `stream_options`

Add import:

```rust
use serde_json::value::RawValue;
```

Extend `RequestHead`:

```rust
#[derive(Deserialize)]
struct RequestHead<'a> {
    #[serde(borrow)]
    model: &'a str,
    #[serde(default)]
    stream: bool,
    #[serde(default, borrow)]
    stream_options: Option<&'a RawValue>,
}
```

After parsing:

```rust
let stream_options_present = head.stream_options.is_some();
```

When building `ProxyContext`, pass:

```rust
stream_options_present,
```

Why this is the right architecture:

- It only uses the parse already required to validate JSON and read `model`/`stream`.
- It checks top-level semantics correctly because serde maps only top-level fields into `RequestHead`.
- It borrows raw JSON and does not allocate/re-encode skipped fields.
- It removes one body-wide memmem scan from proxy hot path.

### 2. In `src/proxy/mod.rs`, add field to `ProxyContext`

```rust
pub struct ProxyContext {
    ...
    pub stream: bool,
    pub stream_options_present: bool,
    ...
}
```

Replace the proxy condition with:

```rust
let request_body = if backend.format == BackendFormat::OpenAi
    && stream_request
    && !ctx.stream_options_present
{
    splice_include_usage(&body)
        .map(Bytes::from)
        .unwrap_or_else(|| body.clone())
} else {
    body.clone()
};
```

Then delete `contains_stream_options()` or keep it only if tests still need it. Prefer deleting it to prevent future misuse.

### 3. Update tests

Add/adjust unit tests so all three cases are locked:

```rust
// root stream_options present: do not inject
{"model":"x","stream":true,"stream_options":{"include_usage":false}}

// string occurrence only: inject
{"model":"x","stream":true,"messages":[{"role":"user","content":"stream_options"}]}

// nested occurrence only: inject
{"model":"x","stream":true,"messages":[{"role":"user","content":"hello","stream_options":{"include_usage":false}}]}
```

At least one integration/runtime smoke should assert the upstream body, not only ledger usage, because a mock backend can return usage even when the request was not injected.

## Validation required

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
python3 /tmp/brigto_stream_options_false_positive.py
```

Expected smoke result after patch:

```text
CASE normal_missing_root status 200 top_has True include_true True
CASE string_false_positive status 200 top_has True include_true True
CASE nested_false_positive status 200 top_has True include_true True
RESULT PASS
```

Then rerun the existing green gates from `CODEX-current-p0-admin-scanner-verified-green-20260916.md` to ensure no regression.

## Non-negotiable

Do not solve this with Redis, DB, middleware, or a second `serde_json::Value` parse in proxy. The minimal SOTA-compatible fix is to carry one top-level bool from the existing borrowed parse in handler into proxy.
