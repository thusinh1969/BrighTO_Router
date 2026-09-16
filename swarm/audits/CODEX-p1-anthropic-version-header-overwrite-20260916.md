# P1 — Anthropic `anthropic-version` client header is overwritten by router

Time: 2026-09-16 22:xx ICT  
Scope: current worktree. Codex did not edit `src/`.

## Verdict

Current Anthropic forwarding violates the cloud/provider passthrough contract. The router forwards `anthropic-beta` and replaces client auth with backend `x-api-key` correctly, but it overwrites the client-provided `anthropic-version` header with static `2023-06-01`.

This can break real Anthropic calls when clients pin a specific API version. It is not a performance optimization and does not need Redis/DB.

## Runtime proof

Command used:

```bash
cargo build --release --locked
python3 /tmp/brigto_anthropic_header_smoke.py
```

Request sent to router:

```text
POST /v1/messages
Authorization: Bearer <client-key>
anthropic-version: 2099-01-01
anthropic-beta: test-beta
```

Mock backend captured:

```text
RESULT status 200
CAPTURE anthropic-version 2023-06-01
CAPTURE anthropic-beta test-beta
CAPTURE x-api-key anthropic-backend-secret
CAPTURE authorization None
FAIL
```

Expected backend capture:

```text
anthropic-version 2099-01-01
anthropic-beta test-beta
x-api-key anthropic-backend-secret
authorization None
```

## Source root cause

`src/proxy/mod.rs::build_reqwest_request()` first copies client headers except secrets/hop-by-hop. Since `anthropic-version` is not dropped, the client value is initially present. Then this branch overwrites it unconditionally:

```rust
BackendFormat::Anthropic => {
    let v = HeaderValue::from_str(key).map_err(|e| format!("invalid backend key: {e}"))?;
    req_headers.insert("x-api-key", v);
    req_headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
}
```

`HeaderMap::insert` replaces existing values. That is why backend sees `2023-06-01` instead of the client’s `2099-01-01`.

## Exact patch

Only set a default Anthropic version if the client did not provide one:

```rust
BackendFormat::Anthropic => {
    let v = HeaderValue::from_str(key).map_err(|e| format!("invalid backend key: {e}"))?;
    req_headers.insert("x-api-key", v);
    if !req_headers.contains_key("anthropic-version") {
        req_headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    }
}
```

Keep `anthropic-beta` passthrough as-is. Do not add it manually; clients may send zero, one, or more beta headers depending on API usage.

Also add/extend a test that locks all four header properties:

1. client `Authorization` is not forwarded;
2. backend `x-api-key` is forwarded;
3. client `anthropic-version` is preserved when provided;
4. default `anthropic-version: 2023-06-01` is inserted only when missing.

If duplicate `anthropic-beta` headers matter for Anthropic beta usage, verify whether current copy path preserves multiple values. If not, make that a separate P2; do not bundle it into this fix.

## Validation

```bash
CARGO_INCREMENTAL=0 cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
python3 /tmp/brigto_anthropic_header_smoke.py
```

Expected smoke output after patch:

```text
CAPTURE anthropic-version 2099-01-01
CAPTURE anthropic-beta test-beta
CAPTURE x-api-key anthropic-backend-secret
CAPTURE authorization None
PASS
```

Then rerun full Postgres tests and previous smokes if proxy code changed.
