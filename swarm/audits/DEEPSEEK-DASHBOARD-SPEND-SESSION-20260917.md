# DeepSeek — dashboard spend/IA + F5 session restore — 2026-09-17

Verdict: Dashboard now shows real spend + top-model/team tables (CODEX dashboard-IA); F5 refresh
restores the admin/user session (CODEX F5 acceptance). Self-audit green; image rebuild in progress.

## Done
- /admin/summary now returns totals.estimated_cost_usd + totals.cost_known_requests, and
  by_model[].estimated_cost_usd (cost = input*price_in + output*price_out over 1M, only when a
  route has both prices). by_team/by_key unchanged this cut.
- Portal Dashboard: cards are Gateway status, Enabled routes, Active providers, Active teams,
  Requests, Total tokens, Estimated cost (real value now, with "N priced requests" or
  "configure route prices" footnote), Error rate. No global P95 card.
- Portal Dashboard: new "Top models" (model/requests/tokens/cost/errors) and "Top teams" tables.
- F5 session restore: browser persists {mode,key,ts} in localStorage with 12h TTL; on refresh the
  portal re-verifies the key via API and re-enters the app; logout clears it. Admin may re-view
  keys as before (reveal endpoint).

## Self-audit (all green)
- cargo fmt + clippy -D warnings, release build, 60 lib + 4 integration tests.
- portal_smoke: PASS (19 checks; summary now asserts cost fields).
- real_provider_smoke: PASS (8 checks; llama.cpp no-auth + DeepSeek + endpoint guard).
- node --check portal JS: PASS.

## Remaining
- Performance diagnostics section (router overhead p95, first-byte/total latency by prompt bucket).
- Provider health table (429/5xx/timeout per provider).
- HTTPS/TLS (Rustls) + start.sh helpers + healthcheck under HTTPS.
- Provider template presets (verified vs experimental) + Anthropic model-list.
