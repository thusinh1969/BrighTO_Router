# DEEPSEEK — Provider/Model UX Redesign Status (RESUME NOTE)

Date: 2026-09-17 (round 8). Written before host reboot for GPU reclaim.
Goal: poll swarm/audits/*, fix until CODEX agrees Playwright 100%. Active.

## Why this round changed course
User directive (Vietnamese): the Provider-type / provider-model-route logic was "confusing/stupid".
"Custom OpenAI" being a provider TYPE forced a Provider with a URL = same as a custom model; making a
model required 2 places. User asked: hard-code the provider list in .env (incl. Custom LLM), Admin just
picks a provider -> fill URL + key -> "Load models" opens a window to PICK ONE model -> "Test connection"
-> only allow Save if passed. Then "xoa DB lam lai" (wipe DB).

## What was implemented (live, deployed)
1. Backend src/admin/mod.rs:
   - PROVIDER_CATALOG env parse -> GET /admin/provider-catalog (entries: key|label|base_url|dialect|key_env|key_set).
   - POST /admin/test-connection: GET /v1/models + optional 1-token completion; returns {ok,latency_ms,status,model_ok,error}.
   - provider_key_ref (env:NAME | file:/path) accepted on UpsertRoute + PreviewModelsRequest; resolve_route_key() helper
     resolves server-side so the browser never needs the .env plaintext.
2. Frontend static/index.html:
   - Unified "Add model" wizard (openRouteModal): Provider(catalog) -> Base URL -> API key -> Provider model +
     "Load models" (opens pickModelModal overlay, search + pick ONE) -> Test connection -> Save (disabled until test ok).
   - Save auto-creates the backend (find-by-base_url or POST) then upserts the route; protocol/auth auto-derived.
   - Local URL + no key -> auth "none" + protocol local_openai_chat (isLocalUrl helper).
   - Provider modal simplified: catalog select (2 dialects) instead of the 12-entry "Provider Type" taxonomy.
   - Removed PROVIDER_TYPES / providerTypeLabel / protocolsForBackend; added catEntry / isLocalUrl.
   - Models button now "Add model"; Providers hint updated.
3. .env: added PROVIDER_CATALOG (openai/anthropic/gemini/deepseek/moonshot/qwen/openrouter/custom).
   NOTE: .env is gitignored; the API_KEY_* values in .env are currently EMPTY placeholders, so catalog key_set=false.
4. DB wiped + seeded: team "Default" (id 1); demo client key lc-0123456789abcdef0123456789abcdef (id 1, owner demo,
   team Default, allowed_models=[], revealable). backends/routes/keys/teams/usage truncated.
5. Rebuilt image (sha256 e9044b24...) + docker compose up -d --force-recreate router. Container healthy.

## Verified
- curl /admin/provider-catalog -> 8 entries (all key_set=false).
- curl /admin/test-connection vs mock -> {"ok":true,"model_ok":true} ; /admin/routes/preview-models works.
- Playwright self-check (swarm/scripts/selfcheck_newflow.mjs): PASS auth, catalog, wizard fields, picker opens.
  Then crashes on a "waiting for visible/enabled/stable" click (same flake CODEX reported).

## IMPORTANT finding (Playwright "not stable" root cause)
- Installed playwright is 1.63.0, which wants chromium revision 1243.
- Local ~/.cache/ms-playwright only has chromium 1234 (older playwright), so every run drives a mismatched browser.
- playwright 1.63.0 CANNOT install chromium on this ubuntu20.04 host ("does not support ... ubuntu20.04-x64").
- The correct browser lives in Docker image mcr.microsoft.com/playwright:v1.63.0-noble (already pulled, 2.51GB).
  => Run Playwright gates INSIDE that image with --network host. This mismatch is very likely why CODEX kept
     seeing "unused Provider Delete is not reliably clickable" — it is a browser artifact, not a CSS bug.
- Canonical gate wrapper swarm/scripts/portal_logic_acceptance.sh picks the local cache chrome-headless-shell (1234),
  so it also runs mismatched. Fix: prefer Docker image, or pin playwright to the version matching chromium 1234.

## How to resume after reboot
1. Docker will auto-restart router + postgres (restart: unless-stopped). Verify: docker ps; curl -sk /admin/... (18443).
2. Restart the local mock upstream (it does NOT auto-start):
   cargo build --bin brighto-router-mock && MOCK_ADDR=127.0.0.1:9000 ./target/debug/brighto-router-mock
3. Run self-check with the correct browser:
   OUT=swarm/out/playwright/selfcheck-docker; mkdir -p $OUT; cp swarm/scripts/selfcheck_newflow.mjs $OUT/
   ADMIN=$(grep '^ADMIN_MASTER_KEY=' .env | cut -d= -f2-)
   docker run --rm --network host -e BRIGHTO_BASE_URL=https://127.0.0.1:18443 -e BRIGHTO_ADMIN_KEY="$ADMIN"      -e BRIGHTO_PW_OUT=/work -e NODE_TLS_REJECT_UNAUTHORIZED=0 -v "$PWD/$OUT:/work"      mcr.microsoft.com/playwright:v1.63.0-noble node /work/selfcheck_newflow.mjs
   (image already has playwright; no local node_modules needed. If playwright import fails, run from inside
    the image's default workdir or npm init + install playwright in $OUT.)
4. REMAINING:
   - Finish verifying the new Add-model flow end-to-end in Docker (picker click -> test -> save -> client smoke).
   - Update swarm/scripts/portal_logic_acceptance.mjs (CODEX canonical gate) to the NEW flow: it still checks
     "Provider Type", "Create model route", "Provider API protocol", seeded 'openai' provider row, etc. — all removed.
     Replace with: catalog check + Add-model wizard + test-connection + save + teams/keys/user (unchanged).
   - Lifecycle-contract UI still pending (provider/model filters, status columns, effective_enabled, enable/disable
     toggles). Backend delete guards for usage-history/model-rename are NOT yet done either.
   - Write a DEEPSEEK note for CODEX describing the redesign so it re-audits the new UX (not the old taxonomy).
