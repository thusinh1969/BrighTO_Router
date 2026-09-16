# CODEX current status — red build + route picker still open — 2026-09-17

User-visible complaint still valid:

```text
Create model route SAI, phải cho refresh lấy list of model về và chọn 1.
Và chưa cho hiển thị key gì cả.
```

Current uncommitted files:

```text
M src/admin/mod.rs
M static/index.html
```

## Build gate is red

Command:

```bash
cargo check --locked --all-targets
```

Current failure: 7 `E0061` call-site errors.

### `query_usage_rows` call-sites wrong

`query_usage_rows(...)` now expects:

```rust
pool, team, key, backend, model, status, from, to
```

But these call-sites still pass the old order/count:

```text
src/admin/mod.rs:1222 get_usage
src/admin/mod.rs:1419 me_usage
```

Fix shape:

```rust
query_usage_rows(
    pool,
    params.team,
    params.key,
    params.backend,
    params.model.as_deref(),
    params.status.as_deref(),
    params.from,
    params.to,
)
```

For user path, keep user isolation:

```rust
query_usage_rows(
    pool,
    None,
    Some(key.id),
    params.backend,
    params.model.as_deref(),
    params.status.as_deref(),
    params.from,
    params.to,
)
```

If `params.backend` is allowed in user portal, it must only filter the authenticated user's own rows because `Some(key.id)` remains forced.

### `push_usage_filters` summary call-sites wrong

`push_usage_filters(...)` now expects:

```rust
builder, alias, team, key, backend, model, &status, from, to
```

But summary still calls old order/count at:

```text
src/admin/mod.rs:1656
src/admin/mod.rs:1670
src/admin/mod.rs:1681
src/admin/mod.rs:1702
src/admin/mod.rs:1724
```

Fix shape:

```rust
let status = normalize_status(params.status.as_deref())?;
let backend = params.backend;
let model = params.model.as_deref();

push_usage_filters(&mut tb,  "",   team, key, backend, model, &status, Some(from), to);
push_usage_filters(&mut pb,  "",   team, key, backend, model, &status, Some(from), to);
push_usage_filters(&mut mb,  "",   team, key, backend, model, &status, Some(from), to);
push_usage_filters(&mut tmb, "u.", team, key, backend, model, &status, Some(from), to);
push_usage_filters(&mut kb,  "u.", team, key, backend, model, &status, Some(from), to);
```

Do not continue UI work until this compiles.

## Route model picker still not implemented

Current `static/index.html` still has manual route form:

```text
Public model name
Provider model name (optional)
Provider backend(s)
```

It does **not** have the required in-modal flow:

```text
Select provider -> Refresh models from provider -> choose exactly one provider model -> save route
```

Provider-table `Load models` is not enough because it bypasses the full route form and creates route directly.

Required UI acceptance:

1. Models & Routes -> Create model route.
2. Select one Provider.
3. Button visible: `Refresh models from provider`.
4. Button calls `GET /admin/backends/{id}/models`.
5. Show searchable model list/dropdown.
6. Selecting one model fills `provider_model_name` and defaults public model alias.
7. Save route with context/max output/prices.
8. Route table shows enough detail to verify selected provider model and prices.

## Key visibility status

One bug is fixed in current diff:

```javascript
keys=await api("/admin/keys","GET"); renderKeys($("content"));
```

This should make the API Keys table refresh after creating a key. Codex will verify by Playwright after build is green.
