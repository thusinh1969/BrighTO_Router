# CODEX audit — Usage filter compile blocker — 2026-09-17

Current uncommitted DeepSeek diff fails compile.

Command run:

```bash
cargo check --locked --all-targets
```

Failure:

```text
error[E0308]: match arms have incompatible types
src/admin/mod.rs:1190-1194
Some("success") => builder.push(" AND status < 400"),
Some("error") => builder.push(" AND status >= 400"),
_ => {}
```

Root cause: `builder.push(...)` returns `&mut QueryBuilder<Postgres>`, while `_ => {}` returns `()`. Rust requires all `match` arms to have compatible types.

## Exact fix shape

Use block arms and terminate `builder.push(...)` with semicolons:

```rust
match status.map(str::trim).filter(|s| !s.is_empty()) {
    Some("success") => {
        builder.push(" AND status >= 200 AND status < 400");
    }
    Some("error") => {
        builder.push(" AND status >= 400");
    }
    Some(raw) => {
        if let Ok(code) = raw.parse::<i64>() {
            builder.push(" AND status = ").push_bind(code);
        }
    }
    None => {}
}
```

Better: normalize lowercase first so `Success`, `ERROR`, and ` 429 ` behave predictably.

## Still needed after compile fix

Do not stop at compile pass. Complete the filter work end-to-end:

1. Add `backend` or `backend_id` to `UsageQuery` and SQL filter.
2. Apply `model`, `status`, `from`, `to` to both admin and user usage paths.
3. Keep user path restricted to `Some(key.id)` so users cannot view another key/team.
4. Update Portal Usage UI with visible filters:
   - date range;
   - provider/backend;
   - model;
   - team;
   - API key;
   - status: all/success/error/numeric.
5. Add smoke tests:
   - `model=smoke-model` returns matching row;
   - `status=success` returns 200 row;
   - `status=error` returns 4xx/5xx row;
   - `status=429` exact works if implemented;
   - user API key cannot filter into another key/team.

Current status: no Playwright retest until `cargo check --locked --all-targets` passes again.
