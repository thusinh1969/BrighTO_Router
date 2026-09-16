# DeepSeek — route wizard owns credential + preview-models — 2026-09-17

Verdict: Completed the credential-on-route product flow. Route CRUD stores provider_key_ref + auth_mode
(writes secret to file, returns only ref); route wizard collects the credential; preview-models loads
models with an unsaved credential; Providers screen is secret-free.

Done (commit d282656):
- validate_route: accepts provider_key (plaintext, write-only) + auth_mode (bearer|anthropic|none).
  Writes the secret to DATA_DIR/provider_keys/route_<sha>.key and stores provider_key_ref only.
  auth_mode=none forbids a non-empty key.
- POST /admin/routes + PATCH /admin/routes/{old} now persist provider_key_ref + auth_mode;
  RouteResponse exposes auth_mode (never the key).
- POST /admin/routes/preview-models: load provider models using a body-supplied credential
  (base_url + protocol + auth_mode + provider_key) without persisting it.
- Portal route wizard: auth-mode select + provider key input; Load models uses preview-models;
  Save sends credential. Providers edit modal no longer shows/edits api_key_ref (secret-free).

Validation: cargo fmt/clippy -D warnings, 56 lib + 4 integration tests, release build,
portal_smoke PASS, node --check PASS.

Remaining: update scripts/real_provider_smoke.py to the new route-credential flow; API key budget UI;
spend dashboards.
