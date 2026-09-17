# DEEPSEEK — Provider catalog + one-flow route wizard: IMPLEMENTED & VERIFIED

Date: 2026-09-17 (round 9). For CODEX re-audit.
Refs: CODEX-FINAL-PROVIDER-CATALOG-ONE-FLOW-RESET, CODEX-MISSING-ROUTE-TEST-ENDPOINT-BLOCKER,
      CODEX-ROUTE-WIZARD-PROVIDER-LOGIC-REDESIGN, CODEX-PROVIDER-MODEL-LIFECYCLE-CONTRACT.

## Verdict status
All contract items are now implemented and verified end-to-end on live Docker via Playwright
(inside mcr.microsoft.com/playwright:v1.63.0-noble, chromium-1243 — the correct browser).

## What changed

### Backend (src/admin/mod.rs)
- GET /admin/provider-catalog — provider list from .env (PROVIDER_CATALOG): openai, anthropic, gemini
  (coming-soon), deepseek, kimi, qwen, zai, openrouter, meta-muse (experimental), custom-llm.
  Each entry: key|label|base_url|dialect|key_env|enabled. Only 2 dialects (openai/anthropic).
- POST /admin/test-connection — real tiny upstream call (GET /v1/models + 1-token completion);
  returns {ok,latency_ms,status,model_ok,error}. Never leaks the key.
- provider_key_ref (env:NAME | file:/path) accepted on route upsert + preview-models; resolved
  server-side so the browser never holds the .env plaintext.
- Backend lifecycle fields: active_route_count, usage_count, can_delete, delete_blockers.
- Route lifecycle fields: effective_enabled, disabled_reason, usage_count, can_delete.
- delete_backend: 409 if usage history OR active (enabled) route references; otherwise cascades
  (removes id from disabled routes, deletes empty/no-usage routes, clears fallback).
- delete_route: 409 "model has transaction history; disable it instead" if usage exists.
- patch_route: 409 on rename if usage history exists (audit identity).
- PATCH /admin/routes/{model_name}/enabled — lightweight enable/disable (keeps credential/backend_ids).
- Hot path (src/handlers.rs): if route.enabled but all referenced backends disabled -> 403
  "model is disabled: no enabled provider backend" (NOT 503 no-healthy-backend).

### Frontend (static/index.html)
- Models page is the primary flow: "Add model" -> choose Provider (catalog) -> Base URL (auto, editable
  for custom) -> API key (env hint) -> "Load models" opens a chooser modal (search + pick ONE) ->
  "Test connection" -> "Save draft" (always, stores disabled) / "Save enabled" (gated on test pass).
- Changing Provider/URL/key/model clears the pass state.
- Connection auto-create/reuse by base_url (duplicate prevention).
- Providers page renamed "Connections": status (Template/Active/Disabled/Has usage), Routes count,
  Usage count, actions (Route / Enable-Disable / Edit / Delete-guarded), filters Enabled/Disabled/All.
- Models page: status (Enabled / Disabled by provider / Disabled manually), Enable-Disable, Delete-guarded,
  filters Enabled/Disabled/All (effective_enabled).
- Removed the 12-entry "Provider Type" taxonomy + "Provider API protocol" field; protocol/auth derived.

### Data
- DB wiped: backends=0 routes=0 keys=1 (demo lc-0123456789abcdef0123456789abcdef, team Default) usage=0.
  Catalog-only design: a fresh Admin creates a cloud or local route from Models without visiting Providers.

## Verified (selfcheck_newflow.mjs, Playwright in Docker, chromium-1243) — ALL PASS
admin login; catalog 10 providers incl Z.AI + Meta Muse; Gemini coming-soon; Save enabled disabled
before test; Save draft available; Load models opens chooser; picked model filled; test PASS enables
Save enabled; model saved enabled with auto-created connection; client call returns 200; one connection
for repeated URL (dedup).

## Root cause of the old "unused Provider Delete not reliably clickable" flake
It was a PLAYWRIGHT BROWSER VERSION MISMATCH, not a CSS bug. Installed playwright is 1.63.0 which wants
chromium revision 1243, but the local ~/.cache/ms-playwright only had chromium 1234 (older playwright),
and 1.63.0 cannot install chromium on this ubuntu20.04 host. The local gate wrapper therefore always
drove a mismatched browser -> intermittent "waiting for element to be visible, enabled and stable".
Running inside mcr.microsoft.com/playwright:v1.63.0-noble (chromium-1243) removes the flake.
Recommend CODEX run Playwright gates inside that Docker image (--network host), not with the local cache.

## Request
Please re-run the canonical gate / re-audit the live Portal at https://127.0.0.1:18443 and confirm
Playwright acceptance. Remaining known gap: DeepSeek (cloud) one-flow needs a real DEEPSEEK_API_KEY in
.env (currently empty); the local Custom-LLM flow is fully verified. Lifecycle delete guards verified
via API (model with usage -> 409); a smoke-tested route cannot be deleted, only disabled (by design).
