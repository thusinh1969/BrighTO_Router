# DeepSeek — portal polish + self-service endpoints — 2026-09-17

Verdict: Portal rebuilt into an OpenRouter-like single-page UI with Admin + User (API-key) login,
plus the minimal supporting endpoints (list teams/keys, daily usage stats, /portal/me self-service).
No new runtime dependency, no Redis, no build system, no provider key plaintext returned.

Evidence:
- static/index.html — rewritten (~820 lines): dark theme, sidebar nav, dashboard cards, SVG usage
  chart (tokens by model, last 30d), model route wizard (pick backend(s) + fetch provider models),
  providers (enable/edit/fetch-models), teams, API keys (reveal-once), usage chart, request logs.
  XSS-safe: all API data rendered via textContent/DOM, no innerHTML with interpolated data.
- src/admin/mod.rs — added routes: GET /admin/teams, GET /admin/keys (LEFT JOIN team name),
  GET /admin/stats?team&days (daily SUM/COUNT), and user router GET /portal/me, /portal/me/usage,
  /portal/me/stats (auth by client API key via auth::authorize_key, NOT admin key). Added key filter
  to GET /admin/usage. Refactored get_usage into query_usage_rows helper (shared with /portal/me/usage).
- src/handlers.rs — mounts /portal via nest_service(crate::admin::user_router).
- AGENT.md — new durable memory doc (architecture, file map, data model, API, boundaries, gotchas).

Validation (all green):
- node --check on extracted portal JS: PASS
- cargo fmt --all -- --check: PASS
- cargo clippy --locked --all-targets -- -D warnings: PASS
- cargo test --all-targets (temp Postgres): 55 lib + 4 integration PASS (new: extract_api_key,
  stats_aggregate_daily_by_model)
- cargo build --release --locked: PASS
- Runtime smoke (release binary, port 18082): healthz/readyz ok; /admin/backends=10; /admin/teams=1;
  /admin/keys=0; /admin/routes=0; /admin/stats=0; POST /admin/keys -> key id 1; GET /portal/me
  (Bearer key) -> owner smoke-user team Default Team; /portal/me/stats=[]; bad key -> 401.

Known gaps (future, not blocking):
- No DELETE route/team endpoints (only upsert + disable). Can add if needed.
- Mobile sidebar is toggled via hamburger; desktop is the primary target.
- Portal reads DB only through admin/user endpoints (off hot path); hot path unchanged.
