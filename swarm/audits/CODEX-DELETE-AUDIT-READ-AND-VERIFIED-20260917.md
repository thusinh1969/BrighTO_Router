# CODEX AUDIT — DeepSeek DELETE note read and verified

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

DeepSeek's DELETE audit was read. Backend DELETE for unused provider is implemented and API smoke passes. This is progress, but browser/product DELETE is not fully accepted until UI and Playwright coverage prove it.

## What DeepSeek reported

File read:

- `swarm/audits/DEEPSEEK-CALL-LOG-COST-CONFIG-RELOAD-DELETE-20260917.md`

DeepSeek claims:

- config reload no longer scans `usage_ledger` every 5 seconds;
- call logs now expose throughput/cost/friendly duration fields;
- `DELETE /admin/backends/{id}` removes unused provider;
- delete returns `409` when provider is still referenced by routes;
- `scripts/portal_smoke.py` asserts enriched usage fields and backend delete.

## What Codex verified locally

Commands run:

```bash
cargo check --locked --all-targets
python3 scripts/portal_smoke.py
```

Result:

- `cargo check`: PASS
- `portal_smoke`: PASS
- `POST /admin/backends`: PASS
- `DELETE /admin/backends` on unused provider: PASS
- enriched `/portal/me/usage` fields: PASS

## Remaining gaps

### 1. Provider DELETE exists in backend but not visible in Providers UI

Current Providers table still shows only:

- Load models
- Edit

No visible Delete/Disable action for provider was observed in `static/index.html` around `renderProviders()`.

DeepSeek must decide product behavior:

- If provider templates can be deleted, add Delete button and test it.
- If provider templates should be preserved and only disabled, label action as Disable and call PATCH enabled=false.

Do not have a backend DELETE endpoint that the Portal cannot exercise.

### 2. Provider DELETE 409 path needs explicit test

`delete_backend()` checks every `model_routes.backend_ids` and returns conflict if referenced. Good design. But smoke currently verifies only the unused-provider success path.

Add test:

1. Create provider.
2. Create route referencing provider.
3. Call `DELETE /admin/backends/{id}`.
4. Expected: `409 Conflict` and message includes route name.
5. Provider still exists.

### 3. Route DELETE needs browser proof

Backend route delete exists:

- `DELETE /admin/routes/{model_name}`

UI has Delete button in Models & Routes, but the user previously reported live delete failure. Do not mark product accepted until Playwright verifies:

1. Create route with unique model name.
2. Click Delete in UI.
3. Confirm dialog.
4. Row disappears.
5. Refresh page.
6. Route remains deleted.
7. API `GET /admin/routes` confirms it is gone.

### 4. API key DELETE is actually Disable

`DELETE /admin/keys/{id}` disables a key. UI label is already `Disable`, which is correct.

Need Playwright proof:

1. Create key.
2. See key row.
3. Click Disable.
4. Row status changes to Disabled.
5. Disabled key cannot call `/portal/me` or model endpoints.

### 5. Team delete/disable remains a product decision

No `DELETE /admin/teams/{id}` was observed. Team edit/disable exists by PATCH.

If Portal offers destructive team action, it must say Disable and use PATCH, or backend must implement safe DELETE with references checked. Do not use misleading labels.

## Acceptance gate

DELETE/disable area is accepted only when all of these pass:

- API smoke: unused provider delete -> 204.
- API smoke: provider in use -> 409.
- Playwright: route delete works and persists after refresh.
- Playwright: key disable works and key becomes unusable.
- Playwright: provider delete/disable button matches actual behavior.
- UI has no button that calls a missing endpoint.

