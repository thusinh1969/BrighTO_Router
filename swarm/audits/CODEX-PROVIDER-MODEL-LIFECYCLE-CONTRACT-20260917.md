# CODEX AUDIT — Provider / Model Lifecycle Contract

Date: 2026-09-17 13:23 ICT  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

Current Provider/Model delete-disable logic must be changed. This is a product contract, not a styling issue.

The Portal must protect production history and avoid destructive actions that are known to fail. PostgreSQL already has the required data: `model_routes` and `usage_ledger`. Do not add Redis for this.

## Definitions

- **Provider** = row in `backends`.
- **Model route / Model** = row in `model_routes`.
- **Provider transaction exists** = at least one row in `usage_ledger` where `usage_ledger.backend_id = backends.id`.
- **Model transaction exists** = at least one row in `usage_ledger` where `usage_ledger.model = model_routes.model_name`.
- **Active model route** = `model_routes.enabled = true` and it references the provider in `backend_ids` or `fallback_backend_id`.
- **Effective enabled model route** = route is manually enabled and at least one referenced provider/backend is enabled. If the provider is disabled, the route is effectively disabled even if `model_routes.enabled = true`.

## Provider rules

### Provider Delete

Provider delete is allowed only when both conditions are true:

1. No active model route references the provider.
2. No transaction log exists for the provider in `usage_ledger`.

If either condition is false, delete must be blocked before a destructive action:

- UI: Delete button disabled or hidden.
- UI should explain why: `Used by 3 active routes` or `Has 128 logged requests`.
- API: return `409 Conflict` with exact reason and list of blocking active routes if applicable.

If delete is allowed:

- Delete the provider row.
- Delete associated model routes that reference only this provider and have no model transactions.
- For routes referencing multiple providers, remove this provider id from `backend_ids`; if no backend remains, delete the route only if it has no transactions, otherwise disable it.
- Also remove `fallback_backend_id` if it equals the deleted provider id.

Do not leave routes pointing to a deleted provider.

### Provider Disable

Provider disable is allowed.

When a provider is disabled:

- Associated model routes must not be usable by clients.
- Do not return `503 no healthy backend available` as the normal UX for a disabled provider. Return a clear model disabled / provider disabled error.
- Portal must show affected routes as disabled by provider.

Minimal no-overengineering implementation:

- Keep `backends.enabled` and `model_routes.enabled` as separate admin intent flags.
- Do not physically set every route to `enabled=false` when provider is disabled, because that loses the difference between manually disabled routes and provider-disabled routes when provider is re-enabled.
- Compute `effective_enabled = route.enabled && any referenced backend.enabled` for UI filters and request admission.
- If a route has all referenced backends disabled, client calls should be rejected as disabled before acquire/forward.

If DeepSeek chooses to physically cascade `model_routes.enabled=false`, it must preserve prior manual state somewhere. That is more schema work and not needed for v1.

## Model route rules

### Model Disable

Model route can be manually disabled at any time.

- UI action: `Disable` / `Enable` instead of forcing Admin to edit modal.
- Disabled model must not be usable by clients.
- Client error should be clear: model disabled.

### Model Delete

Model route delete is allowed only when no transaction log exists for that model:

```sql
SELECT COUNT(*) FROM usage_ledger WHERE model = $model_name;
```

If count > 0:

- UI Delete disabled or hidden.
- API returns `409 Conflict` with `model has transaction history; disable it instead`.
- Admin can still disable the model.

If count = 0:

- Delete is allowed, even if it is active, after confirm.
- Route must disappear from UI and `/admin/routes`.

Recommended smart guard:

- If a model has transactions, do not allow renaming `model_name`, because `usage_ledger.model` is the audit identity. Allow editing metadata such as provider model, pricing, context, max output, timeout, enabled.

## Portal filters

Add simple filters to Provider and Model pages:

### Provider page filter

Options:

- `Enabled only`
- `Disabled only`
- `All`

Provider status columns should include:

- Enabled / Disabled.
- Active routes count.
- Transaction count or `Has usage` boolean.

Actions:

- `Edit`
- `Disable` or `Enable`
- `Delete` only enabled when safe by the rules above.

### Model page filter

Options:

- `Enabled only`
- `Disabled only`
- `All`

For models, filter by **effective enabled**, not only raw `model_routes.enabled`.

Status examples:

- `Enabled`
- `Disabled manually`
- `Disabled by provider: qwen`
- `Disabled: no enabled provider`

Actions:

- `Edit`
- `Disable` or `Enable`
- `Delete` only enabled when model has no transaction history.

## Backend/API requirements

Add lightweight admin summary fields; no heavy schema required:

For `/admin/backends`, include or compute in UI from existing endpoints:

```json
{
  "active_route_count": 2,
  "usage_count": 128,
  "can_delete": false,
  "delete_blockers": ["active routes: test-qwen", "usage history exists"]
}
```

For `/admin/routes`, include or compute in UI:

```json
{
  "effective_enabled": false,
  "disabled_reason": "provider disabled: qwen",
  "usage_count": 128,
  "can_delete": false
}
```

Implementation can compute these in Rust SQL using existing tables. Keep it simple. Do not introduce Redis, background jobs, or a separate registry service.

## SQL checks to implement

Provider active route check:

```sql
SELECT model_name, backend_ids, fallback_backend_id
FROM model_routes
WHERE enabled = true;
```

Then parse `backend_ids` JSON text with existing `parse_backend_ids()` and match provider id, plus `fallback_backend_id`.

Provider transaction check:

```sql
SELECT COUNT(*) FROM usage_ledger WHERE backend_id = $1;
```

Model transaction check:

```sql
SELECT COUNT(*) FROM usage_ledger WHERE model = $1;
```

## Acceptance tests DeepSeek must run before saying done

Use live Docker after rebuild/recreate.

### Provider lifecycle

1. Create provider `tmp-provider-a`.
2. Create route `tmp-model-a` pointing to it.
3. Confirm Provider Delete is disabled/blocked because active route exists.
4. Disable route `tmp-model-a`.
5. Confirm Provider Delete is allowed if provider has zero `usage_ledger` rows.
6. Delete provider and confirm associated no-history route is removed or cleaned so no route references deleted backend id.
7. Create provider `tmp-provider-b` and route `tmp-model-b`.
8. Send one request through `tmp-model-b` so `usage_ledger.backend_id = tmp-provider-b.id` and `usage_ledger.model = tmp-model-b` exist.
9. Disable route.
10. Confirm Provider Delete is blocked because provider has transaction history.
11. Confirm Model Delete is blocked because model has transaction history.
12. Confirm both can be disabled.

### Provider disable cascade

1. Disable `custom-local-llama` provider.
2. `test-custom-local` must appear under `Disabled only` in Models.
3. Client call to `test-custom-local` must fail with clear disabled/provider-disabled error, not normal backend 503.
4. Re-enable provider.
5. If route itself was manually enabled, `test-custom-local` becomes effectively enabled again.
6. If route was manually disabled, provider re-enable must not re-enable it.

### Filters

1. Provider `Enabled only` shows only enabled providers.
2. Provider `Disabled only` shows only disabled providers.
3. Provider `All` shows both.
4. Model `Enabled only` uses effective enabled.
5. Model `Disabled only` includes manually disabled and provider-disabled routes.
6. Model `All` shows both.

## Current related live findings

After current seed, provider list is clean and in-use provider delete is disabled in UI. Remaining blockers from latest live probe:

- Provider Delete after edit is not reliably clickable.
- Model Route Delete via UI leaves the route behind.
- Cloud routes are poisoned by unauthenticated background health checks and later return `503 no healthy backend available`.

These must be fixed together with the lifecycle contract above.
