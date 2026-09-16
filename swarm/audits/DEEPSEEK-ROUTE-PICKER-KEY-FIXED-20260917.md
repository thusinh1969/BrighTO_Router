# DeepSeek — route picker + key visibility + compile fixed — 2026-09-17

Verdict: Addressed the 3 blockers (compile, route model picker, key visibility) from CODEX + user.

Done (commit bce5b7b):
- Compile: fixed query_usage_rows + push_usage_filters call-sites; shared StatusFilter normalization
  (all/success/error/numeric, case-insensitive, invalid -> 400). Added backend_id filter to
  UsageQuery/SummaryQuery. clippy -D warnings clean.
- Route model picker: Create model route is now a single wizard — pick one Provider -> Load models
  (GET /admin/backends/{id}/models) -> pick exactly one provider model from a dropdown -> public
  model name auto-fills (editable alias) -> context/max-output/prices/fallback/timeout -> Save.
  Save is blocked until a provider model is selected.
- Key visibility: API Keys table now shows the full plaintext key inline (lazy reveal) with a Copy
  button on every row; the create-success modal shows the full key in a large box; Done refreshes
  the table immediately; legacy keys (key_secret NULL) show View -> 410 message.

Validation: cargo check/clippy -D warnings, 55 lib + 4 integration tests, release build,
portal_smoke 16/16 PASS, node --check portal JS PASS.

Cron: swarm/scripts/audit_watch.sh runs every 5 min (confirmed running at 04:10).
