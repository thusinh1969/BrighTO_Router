# CODEX audit — Usage filter call-site compile blocker — 2026-09-17

Current uncommitted files:

```text
M src/admin/mod.rs
M static/index.html
```

Command:

```bash
cargo check --locked --all-targets
```

Current failure changed from the old match-arm error to call-site errors:

```text
error[E0061]: this function takes 9 arguments but 8 arguments were supplied
src/admin/mod.rs:1676, 1690, 1701, 1722, 1744
```

## Root cause

`push_usage_filters(...)` now has this shape:

```rust
fn push_usage_filters(
    b,
    alias,
    team,
    key,
    backend,
    model,
    status: &StatusFilter,
    from,
    to,
)
```

But `/admin/summary` still calls it like this:

```rust
push_usage_filters(&mut tb, "", team, key, model, status, Some(from), to);
```

So `model` is being passed in the backend slot, and the required `&StatusFilter` argument is missing.

## Exact fix shape

Inside `get_summary`, normalize status once:

```rust
let status = normalize_status(params.status.as_deref())?;
let backend = params.backend;
let model = params.model.as_deref();
```

Then update every summary call-site:

```rust
push_usage_filters(&mut tb, "", team, key, backend, model, &status, Some(from), to);
push_usage_filters(&mut pb, "", team, key, backend, model, &status, Some(from), to);
push_usage_filters(&mut mb, "", team, key, backend, model, &status, Some(from), to);
push_usage_filters(&mut tmb, "u.", team, key, backend, model, &status, Some(from), to);
push_usage_filters(&mut kb, "u.", team, key, backend, model, &status, Some(from), to);
```

Also add `backend: Option<i64>` to `SummaryQuery` if provider/backend filtering is part of this cut.

## Do not proceed to UI/browser until green

Required gate before any Playwright retest:

```bash
cargo check --locked --all-targets
python3 scripts/hotpath_guard.py
python3 scripts/portal_smoke.py
```

## Separate UI blocker still open

The live user feedback about route creation is still not fixed in current `static/index.html`:

- `Create model route` still has no button inside the modal to refresh/load provider models.
- Admin still cannot select exactly one provider model from a loaded list inside that route flow.
- Provider-table `Load models` remains a separate route shortcut and does not replace the required route wizard.
