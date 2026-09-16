# CODEX AUDIT — Edit CRUD identity blocker

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

FAIL for production portal CRUD until edit flows preserve the original record identity.

This is not cosmetic. The user reproduced the route bug live: open an existing model route, edit it, press Save, and the portal creates a new row instead of updating the intended row.

## Root cause

`model_routes` uses `model_name` as the primary key. The current portal edit modal lets the admin change `model_name`, then submits only:

```json
{
  "model_name": "new-public-name",
  "backend_ids": [...],
  "provider_model_name": "..."
}
```

The backend endpoint is `POST /admin/routes` and performs an upsert on the submitted `model_name`. If the admin changed the public model name, PostgreSQL sees a new primary key and inserts a second route. There is no `original_model_name` or dedicated update endpoint, so the backend cannot know this was meant to rename/update the old route.

## Required one-pass fix

DeepSeek should fix the route edit contract, not patch around it in the DOM.

Use one of these concrete options:

1. Preferred: add `PATCH /admin/routes/{old_model_name}`.
   - Path param identifies the existing row.
   - JSON body may include the new `model_name`, selected backend, provider model, context, max output, prices, fallback, timeout, enabled.
   - If `model_name` changes, update the primary key row in one transaction.
   - Return `404` if old route does not exist.
   - Return `409` if the new model name already belongs to a different route.

2. Acceptable short-term: keep `POST /admin/routes`, but portal must pass `original_model_name` and backend must update/delete correctly.
   - This is less REST-clean and easier to misuse later.

Do not rely on frontend deletion + create as the normal edit path. That creates data-loss and race risks, especially when keys are using `allowed_models`.

## Acceptance tests

Run these exact checks after the fix:

1. Create route `route-a` to backend X.
2. Edit route without changing public model name.
   - Expected: still exactly one `route-a` row.
3. Edit route and rename public model to `route-b`.
   - Expected: `route-a` no longer exists; exactly one `route-b` exists.
4. Try renaming `route-b` to an existing `route-c`.
   - Expected: `409 Conflict`, no duplicate rows.
5. UI Playwright check:
   - open Models & Routes
   - edit an existing route
   - save after changing provider model/context/price
   - table row count remains unchanged
   - save after changing public model name
   - table row count remains unchanged and row label changes

## Also audit these edit flows in the same pass

The user explicitly requested checking all edit flows:

- Team edit: must update the same team id, not create a new team.
- Provider edit: must update the same backend id, not create a new backend.
- API key edit/disable: current portal appears to support disabling, not full edit. If full edit is intentionally unsupported, label it clearly as Disable, not Edit. If edit is required, add `PATCH /admin/keys/{id}` with owner/limits/budget/expiry/allowed models/enabled.


## Live user report: delete action also failing

After this audit was created, the user reported from the live portal: “Delete cũng ko được”. DeepSeek must not treat CRUD as fixed until delete is tested in the real browser.

Required delete checks:

1. Model route delete:
   - Create a unique route.
   - Click Delete in portal.
   - Confirm browser dialog if present.
   - Expected: row disappears after refresh and `GET /admin/routes` no longer contains the route.
   - If the row stays visible, inspect whether the UI forgot to refresh, delete endpoint failed, model name path encoding is wrong, or modal/table state is stale.

2. Provider delete:
   - There is currently no obvious `DELETE /admin/backends/{id}` endpoint in the admin router.
   - If the UI shows delete for provider, it cannot work correctly until backend supports it or UI labels it as Disable.
   - Do not hard-delete a provider referenced by routes without either blocking with a clear message or cascading intentionally.

3. Team delete:
   - There is currently no obvious `DELETE /admin/teams/{id}` endpoint in the admin router.
   - If UI shows delete, it must either disable the team or backend must implement safe delete rules.

4. API key delete/disable:
   - API endpoint is `DELETE /admin/keys/{id}` and should disable, not remove.
   - UI must show the row becomes disabled after clicking.
   - If the user sees no effect, fix frontend refresh/state or error toast visibility.

Production rule: every destructive button must be backed by a tested API endpoint and a visible post-action state change. If the backend only supports disable, the button text must say `Disable`, not `Delete`.
