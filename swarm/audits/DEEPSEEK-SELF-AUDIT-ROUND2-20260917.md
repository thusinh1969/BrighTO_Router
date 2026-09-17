# DeepSeek — self-audit round 2: bugs found & fixed, full Playwright clean — 2026-09-17

Verdict: Ran a real self-audit (Playwright on temp router + live https portal + real-provider
smokes) and fixed the bugs it surfaced. Prior 'done' was premature; this round closes the gaps.

## Bugs found and fixed
1. Provider edit was buggy: provider modal used a text input for format and hardcoded
   api_key_ref='env:NONE' on edit (silently resetting the key ref). Fixed: format is now a
   select (OpenAI-compatible / Anthropic Messages); api_key_ref is only set on create.
2. Stale render after mutations: deleteProvider/deleteRoute/disableKey/etc. called renderX()
   directly without clearing #content, so old rows stayed after delete/edit. Fixed: every
   renderX() now clears its container first. (Delete provider now removes the row immediately.)
3. API Keys view emitted a 410 console error per legacy key (key_secret NULL) by auto-revealing
   every key. Fixed: /admin/keys now returns revealable=false for legacy keys; UI only
   auto-reveals revealable keys and shows 'legacy · recreate' otherwise.
4. Row spacing too tall: table cell padding 11px -> 7px vertical for denser tables.
5. Copy button: replaced the 'Copy' text buttons with square icon-only buttons.

## TLS
- Live portal now serves HTTPS on 0.0.0.0:18443 (self-signed cert from ssl/ mounted to /certs;
  container healthy). HTTP 18080 retired.
- Temp-router test harnesses (portal_smoke / real_provider_smoke / playwright runner) now set
  TLS_CERT_PATH/TLS_KEY_PATH empty so they no longer accidentally pick up .env TLS and fail.

## Self-audit results (all green)
- Playwright comprehensive (temp HTTP router): PASS, 0 failures — login, all 6 views, provider
  create+edit+delete, route create+edit+delete, team create, key create+reveal, usage filters,
  user mode, mobile 390px.
- Playwright live (https://127.0.0.1:18443, ignoreHTTPSErrors): AUDIT CLEAN, 0 errors/0 failures.
- portal_smoke 19 checks: PASS. real_provider_smoke 8 checks: PASS.
- NEW scripts/anthropic_smoke.py: PASS — anthropic_messages route via Claude (claude-opus-5,
  x-api-key) + endpoint guard (anthropic route rejects /v1/chat/completions -> 400).
- cargo fmt + clippy -D warnings, release build, 60 lib + 4 integration tests: PASS.

## Remaining (minor, not blockers)
- Provider (backend) key reveal endpoint for legacy env:/file refs — route credential is
  write-only by design; if admin needs to re-view provider keys, add a reveal endpoint.
