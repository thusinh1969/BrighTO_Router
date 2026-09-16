# DeepSeek — config reload no longer scans ledger + real-provider smoke self-contained — 2026-09-17

Verdict: Addressed the CODEX config-reload audit (PostgreSQL poll must touch only config
tables). Real-provider smoke now runs isolated and passes end-to-end for keyless llama.cpp
and route-level-credential DeepSeek V4 Pro. API key budget got a friendly form.

Done:
- CODEX config-reload fix: removed the usage_ledger COUNT/SUM query from
  DbConfigLoader::load_snapshot(). It ran on EVERY 5s poll (boot counter misplaced), which
  would scan the usage table every reload in production. New log_usage_boot_counter() runs
  exactly once at boot (main.rs), logs totals, and errors are non-fatal. Poll path now
  queries only backends/model_routes/teams/api_keys.
- scripts/real_provider_smoke.py rewritten self-contained: free ports + temp Postgres +
  migrations + explicit DATABASE_URL/ADMIN_MASTER_KEY/DATA_DIR. No longer relies on dotenv
  -> dev DB, no hardcoded port 18088 collision. Router log tail printed on failure.
- Portal: API key modal now has friendly budget controls (Inherit team / Token budget +
  period + amount) with Advanced JSON collapsed for money budget (max_usd_cents) and
  per-model caps.

Validation: cargo fmt/clippy -D warnings, release build, 56 lib + 4 integration tests,
portal_smoke PASS (16), real_provider_smoke PASS (7: local llama.cpp keyless chat +
DeepSeek preview-models + route credential + chat). DeepSeek key never printed.

Remaining: spend dashboards (ledger cost fields + hot-path cost from route price);
DELETE /admin/backends/{id} + provider edit identity.
