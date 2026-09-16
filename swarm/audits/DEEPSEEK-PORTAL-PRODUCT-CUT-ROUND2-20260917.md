# DeepSeek — portal product cut round 2 — 2026-09-17

Verdict: Addressed CODEX key-reveal P0s and the route pricing/context product gap. Remaining gaps
(usage filters/grouped stats, team money budget, ledger cost, provider test-connection, DELETE route)
are tracked and deferred to the next rounds.

Done this round (commit e96ce8a):
- Key-reveal P0s: removed "shown once" wording (UI + admin comment + smoke); API Keys table has
  View key (reveal) + Disable; smoke now has negative cases (bad admin 401, client key 401,
  legacy key_secret NULL -> 410). 15/15 smoke PASS.
- SECURITY.md documents the key_secret plaintext tradeoff + encryption guidance for enterprise.
- migrations/0004_route_pricing_and_context.sql: model_routes gains provider_model_name,
  context_tokens, max_output_tokens, price_input_per_mtok_usd, price_output_per_mtok_usd, enabled.
- Route GET/POST expose + validate the new fields (prices >= 0). Route modal in portal has provider
  model name, context window, max output tokens, price per 1M input/output.
- API key modal now has expiry date, requests/minute, concurrency limit, budget JSON.
- Cron: removed stale scripts/audit_watch.sh entry; swarm/scripts/audit_watch.sh remains (poll 5 min).

Validation (all green): node --check, fmt --check, clippy -D warnings, 55 lib + 4 integration tests,
release --locked build, portal_smoke 15/15.

Remaining (next rounds):
- Usage filters (date/provider/model/team/key/status) + grouped stats + error rate + p95.
- Team friendly budget UI (unlimited/token/money) + Budget.max_usd_cents + money enforcement.
- usage_ledger cost fields + spend dashboards (needs hot-path cost from route price).
- Provider test-connection + provider-key update flow in portal.
- DELETE /admin/routes/{model_name}.
