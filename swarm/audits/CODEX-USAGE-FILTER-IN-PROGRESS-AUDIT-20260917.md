# CODEX audit — Usage filters in progress, do not commit half-way — 2026-09-17

Current uncommitted diff seen by Codex:

```text
M src/admin/mod.rs
```

Diff adds to `UsageQuery`:

```rust
model: Option<String>,
status: Option<String>,
```

But `query_usage_rows(...)` still accepts only:

```rust
team, key, from, to
```

and `get_usage(...)` still calls:

```rust
query_usage_rows(pool, params.team, params.key, params.from, params.to)
```

## Required root-cause fix

Do not ship only the struct fields. Wire the filters end-to-end:

1. Update `query_usage_rows` signature to accept `model` and `status`.
2. Add SQL filters:
   - `model = $x` for exact model;
   - status class mapping is clearer than raw text only:
     - `success` -> `status >= 200 AND status < 400`
     - `error` -> `status >= 400`
     - `all` or empty -> no status filter
     - optional numeric status like `429` -> `status = 429`
3. If adding provider/backend filter, use `backend_id`, not provider name, then the UI can map name -> id.
4. Update `GET /admin/usage` call site.
5. Keep `/portal/me/usage` restricted to the authenticated key even when model/status/from/to filters are added.
6. Add smoke coverage for at least:
   - model filter returns matching row only;
   - success/error status filter works;
   - user portal cannot filter into another key/team.

## UI expectation

Usage screen must expose visible filters:

```text
Date range | Provider | Model | Team | API key | Status
```

Then show totals for the filtered result:

```text
Requests | Input tokens | Output tokens | Cost if known
```

No fake cost. Use `—` when price data is missing.
