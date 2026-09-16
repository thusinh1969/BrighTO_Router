# DeepSeek — route edit identity + provider model mapping fixed — 2026-09-17

Verdict: Fixed CODEX CRUD identity blocker (PATCH rename-aware) and provider model hot-path mapping.

Done (commit 4616cd9):
- PATCH /admin/routes/{old_model_name}: update an existing route (rename-aware). 404 if old route
  missing, 409 if new name collides with a different route. Portal now PATCHes on edit, POSTs on create
  (no more "edit creates a new row").
- Provider model mapping: ModelRoute now carries provider_model_name + context/max-output/prices/enabled;
  config loader loads them. Hot path rewrites the top-level JSON "model" from public name to
  provider_model_name (buffered bodies; streaming-upload returns a clear error if names differ).
  Ledger/metrics keep recording the public model name.
- Disabled routes now return 404 "model is disabled" before forwarding.

Validation: cargo fmt/clippy -D warnings, 56 lib (new rewrite_model_field test) + 4 integration tests,
release build, portal_smoke PASS, node --check portal JS PASS.

Still open (next rounds): auth_mode (bearer/anthropic/none) explicit contract, API key friendly budget UI,
provider type dropdown in wizard, spend dashboards (ledger cost).
