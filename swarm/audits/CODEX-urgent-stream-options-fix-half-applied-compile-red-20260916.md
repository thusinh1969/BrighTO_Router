# URGENT — `stream_options_present` fix is half-applied; compile is red

Time: 2026-09-16 22:xx ICT  
Scope: current worktree. Codex did not edit `src/`.

## Verdict

DeepSeek started the correct P1 fix in `src/handlers.rs`, but stopped before updating `src/proxy/mod.rs`. Current source does **not compile**.

This is a small incomplete refactor, not a reason to revert to body-wide memmem. Finish the bool handoff into `ProxyContext`.

## Current compile failure

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
```

Output:

```text
error[E0560]: struct `ProxyContext` has no field named `stream_options_present`
   --> src/handlers.rs:182:9
    |
182 |         stream_options_present,
    |         ^^^^^^^^^^^^^^^^^^^^^^ `ProxyContext` does not have this field
```

## Current source state

`src/handlers.rs` is already correct on the producer side:

```text
src/handlers.rs:15  use serde_json::value::RawValue;
src/handlers.rs:126 let stream_options_present = head.stream_options.is_some();
src/handlers.rs:182 stream_options_present,
```

But `src/proxy/mod.rs` still has old consumer side:

```text
src/proxy/mod.rs:253 pub struct ProxyContext {
src/proxy/mod.rs:258     pub stream: bool,
src/proxy/mod.rs:259     pub reservation: Option<BudgetReservation>,
...
src/proxy/mod.rs:647     && !contains_stream_options(&body)
```

## Exact patch required

### 1. Add the field to `ProxyContext`

In `src/proxy/mod.rs`:

```rust
pub struct ProxyContext {
    pub api_key: ApiKey,
    pub model_name: String,
    pub route: ModelRoute,
    pub request_id: String,
    pub stream: bool,
    pub stream_options_present: bool,
    pub reservation: Option<BudgetReservation>,
    pub concurrency: Option<ConcurrencyGuard>,
    pub start: Instant,
    pub estimated_input_tokens: u64,
}
```

### 2. Use the field in injection gate

Replace:

```rust
&& !contains_stream_options(&body)
```

with:

```rust
&& !ctx.stream_options_present
```

### 3. Remove or quarantine old memmem helper

Delete `contains_stream_options()` and update the unit test that currently asserts only the top-level-positive case. The old helper is the root cause of the false positive. Keeping it around as a public-looking helper invites regression.

If you keep a unit test for splice, test the decision via a small helper that takes `stream_options_present: bool`, not by scanning bytes.

## Validation required

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
python3 /tmp/brigto_stream_options_false_positive.py
```

Expected smoke after the completed fix:

```text
CASE normal_missing_root status 200 top_has True include_true True
CASE string_false_positive status 200 top_has True include_true True
CASE nested_false_positive status 200 top_has True include_true True
RESULT PASS
```

Then rerun P0 smokes:

```bash
python3 /tmp/brigto_invalid_json_smoke.py   # expected printed status 400 hits 0
python3 /tmp/brigto_p0_smoke.py             # expected FAILURES 0
```
