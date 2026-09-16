# DeepSeek — portal SOTA redesign (matches Codex mock) — 2026-09-17

Verdict: Portal redesigned to match the Codex mock direction (navy #0f172a/#0b1220, sky->violet
gradient, blue primary, Inter, rounded cards), plus admin username/password login, key re-view for
Admin (user-directed), and new endpoints (POST /admin/backends, GET /admin/keys/{id}/reveal,
GET /admin/settings). No build system, no Redis, no provider plaintext leak in list endpoints.

Evidence:
- static/index.html — rewritten (~709 lines) to the mock visual direction. Nav: Dashboard,
  Providers, Models & Routes, Teams, API Keys, Usage, Settings. Dashboard cards + tokens-by-model
  chart + recent requests. Providers (add/edit/load-models), routes wizard, teams, keys (create +
  View key + disable), usage chart + logs, settings (read-only). Login = username "admin" +
  password ADMIN_MASTER_KEY (admin) or client API key (user). XSS-safe rendering (textContent/DOM).
- migrations/0003_admin_key_reveal.sql — api_keys.key_secret TEXT (plaintext stored so Admin can
  re-view client keys, per user request). Only keys created AFTER this migration store it; older
  keys -> reveal returns 410 with a clear message.
- src/admin/mod.rs — create_key now stores key_secret; new GET /admin/keys/{id}/reveal (admin only),
  GET /admin/settings (read-only runtime info), POST /admin/backends (add provider). List endpoints
  (/admin/keys, /admin/backends) still never return plaintext.

Security note: storing client key plaintext is a deliberate user-directed tradeoff (self-hosted
admin). List endpoints do NOT leak it; only the explicit admin reveal endpoint returns it.

Validation (all green):
- node --check portal JS: PASS
- cargo fmt --all -- --check: PASS
- cargo clippy --locked --all-targets -- -D warnings: PASS
- cargo test --all-targets (temp Postgres): 55 lib + 4 integration PASS
- cargo build --release --locked: PASS
- python3 scripts/portal_smoke.py: 12/12 PASS (now includes reveal, settings, create backend)

Remaining for full product (future rounds, per CODEX-PORTAL-SOTA-PRODUCT-DESIGN):
- model_routes pricing/context columns + money budget + ledger cost fields (spend dashboards).
- stats grouped by provider/model/team/key + error rate + p95 from ledger.
- provider test-connection + provider-key update flow in portal.
- DELETE /admin/routes/{model_name}.
