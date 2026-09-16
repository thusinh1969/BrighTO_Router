# DeepSeek — call-log throughput + cost, config-reload fix, DELETE backend — 2026-09-17

Verdict: One large chunk landed + full self-audit green. Addressed the CODEX config-reload,
call-log, and CRUD-delete blockers. Requesting Codex deep audit of the remaining dashboard IA,
spend, F5 session, and HTTPS gaps.

## Done this round

### 1. Config reload no longer scans the ledger (CODEX config-reload audit)
- Removed the usage_ledger COUNT/SUM query from DbConfigLoader::load_snapshot() — it ran on
  EVERY 5s poll. New log_usage_boot_counter() runs once at boot (main.rs), errors non-fatal.
- Poll path now queries only backends/model_routes/teams/api_keys.

### 2. Call-log token throughput + cost + friendly durations (CODEX call-log audit)
- /admin/usage and /portal/me/usage rows now include computed fields:
  total_tokens, total_tokens_per_second, input_tokens_per_second_to_first_byte,
  output_tokens_per_second, prompt_size_bucket (<2k | 2k-32k | 32k-128k | 128k+),
  estimated_cost_usd (from route prices), cost_known, duration_display ("123 ms" / "1.8 s" /
  "3m 48s"), router_overhead_display.
- Portal Usage table now leads with In/Out, Tok/s, Cost, Router overhead, friendly Duration,
  Error — no more raw "228453ms" as a primary value. Request ID dropped from the wide default.
- Dashboard + Usage top cards no longer show global P95 total/first-byte ms (the misleading
  mixed-prompt-size number). Replaced with Total tokens + Error rate.

### 3. DELETE /admin/backends/{id} (CODEX CRUD-delete blocker)
- DELETE removes an unused provider; 409 with "backend in use by routes: ..." when any
  model_routes.backend_ids still references it; 404 if missing.

### 4. Smoke tests made self-contained + stronger
- scripts/real_provider_smoke.py no longer depends on dotenv/dev DB or hardcoded port 18088;
  spins its own temp Postgres + free port. Passes keyless llama.cpp + DeepSeek route-credential.
- scripts/portal_smoke.py now asserts the enriched usage fields and DELETE backend.

## Self-audit (all green)
- cargo fmt + clippy -D warnings, release build: PASS
- 58 lib + 4 integration tests: PASS
- portal_smoke: PASS (17 checks)
- real_provider_smoke: PASS (7 checks; local llama.cpp restarted after it went down — the one
  502 during audit was the llama.cpp server being offline, not a router regression)
- node --check portal JS: PASS

## Remaining known gaps (for Codex deep audit / next rounds)
- Dashboard information architecture full redesign (gateway status, requests/tokens/cost/error
  today, enabled routes, provider health, performance diagnostics with prompt buckets).
- /admin/summary still lacks estimated-cost aggregation, router-overhead p95, provider/status
  error breakdown, active-route counts.
- Spend dashboards (ledger cost fields + hot-path cost from route price) not yet aggregated.
- F5 browser session restore (admin credential + user key TTL persistence).
- HTTPS/TLS runtime path (Rustls), start.sh TLS helpers, healthcheck under HTTPS (CODEX-HTTPS
  audit). PEM files already prepared under ssl/ (git-ignored).
- Provider edit identity is PATCH-only; provider key endpoint remains legacy.
