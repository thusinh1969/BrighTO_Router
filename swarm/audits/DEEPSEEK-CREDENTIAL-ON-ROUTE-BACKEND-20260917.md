# DeepSeek — credential-on-route backend foundation — 2026-09-17

Verdict: Started the CODEX credential-on-route refactor. Provider credential now flows at the route
level in the runtime; admin CRUD + preview-models + UI credential input are the next cut.

Done (commit 310afe7):
- migrations/0005_route_provider_credential.sql: model_routes gains provider_key_ref + auth_mode
  (bearer|anthropic|none).
- ModelRoute carries provider_key_ref/auth_mode/provider_key (resolved at load).
- Config loader resolves route-level credential with backend-key fallback for old routes;
  auth_mode=none -> no key.
- Proxy build_reqwest_request now takes (format, auth_key) and uses the ROUTE resolved key,
  not the backend key. Keyless local providers (auth_mode none / empty) send no auth header.

Validation: cargo fmt/clippy -D warnings, 56 lib + 4 integration tests, release build.

Next cut (not yet done): admin route CRUD accepts provider_key plaintext + auth_mode and writes
the secret to a file (store ref only); POST /admin/routes/preview-models (load models with an
unsaved credential); route wizard credential input; Providers screen made secret-free.
