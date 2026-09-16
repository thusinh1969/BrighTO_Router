# CODEX audit delta — Usage summary added while build is still red — 2026-09-17

Current uncommitted diff: `src/admin/mod.rs`.

`cargo check --locked --all-targets` still fails with the same compile blocker:

```text
error[E0308]: match arms have incompatible types
src/admin/mod.rs:1191-1194
```

## Root cause still present

`query_usage_rows` still has:

```rust
match status {
    Some("success") => builder.push(" AND status < 400"),
    Some("error") => builder.push(" AND status >= 400"),
    _ => {}
}
```

Those first two arms return `&mut QueryBuilder`, while `_` returns `()`. This must be fixed before adding more dashboard work.

## New issue introduced by current diff

A helper `push_usage_filters(...)` was added for `/admin/summary`, but `query_usage_rows(...)` still implements its own separate filter logic. That creates two filter implementations that can drift.

Use one shared helper or one shared status-normalization function so these endpoints behave identically:

```text
GET /admin/usage
GET /portal/me/usage
GET /admin/summary
```

## Required implementation sequence

1. Fix compile in `query_usage_rows` immediately.
2. Normalize status once:
   - empty/all -> no filter;
   - success -> `status >= 200 AND status < 400`;
   - error -> `status >= 400`;
   - numeric string, for example `429` -> `status = 429`;
   - invalid status -> HTTP 400 with a clear message, not silently ignored.
3. Add backend/provider filter if this is the final Usage filter cut.
4. Apply same filter semantics to usage rows and summary.
5. Only then update Portal Usage UI and smoke tests.

Current gate: no Playwright retest while `cargo check --locked --all-targets` is red.
