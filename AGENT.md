# AGENT.md — BrighTO Router (durable memory for agent resume)

> Auto-resume cheat sheet. Read this first after any restart/fork. Keep it updated as the repo evolves.
> Last updated: 2026-09-17 (portal rebuild complete).

## What this repo is

**BrighTO Router** — a fast Rust LLM reverse proxy + usage meter in front of OpenAI/Anthropic-compatible
providers (OpenAI, Anthropic, Gemini, DeepSeek, OpenRouter, ...). Authenticates client API keys, enforces
per-team/key budgets + rate limits + concurrency, routes each model to backends (fallback + circuit breaker),
streams responses, and records usage to a PostgreSQL ledger. Ships an embedded admin + user portal.

Public repo: https://github.com/thusinh1969/Brighto_Airouter . Docker image: thusinh1969/brighto_airouter:v1 .

## Stack (do not drift)

- Rust edition 2024 (MSRV 1.94). axum 0.8, sqlx 0.9 PostgreSQL-only, reqwest 0.13 (rustls=aws-lc-rs, ONE crypto
  provider), tokio, arc-swap, dashmap, metrics 0.24 + metrics-exporter-prometheus, serde_json RawValue.
- No Redis in default runtime. Portal is a single static static/index.html (HTML/CSS/JS, no CDN/build system).
- Config loaded from Postgres at boot + polled every 5s (or on admin notify) into ArcSwap<ConfigSnapshot>.
  Hot path reads ONLY that RAM snapshot.

## Architecture

- Hot path (proxy, stays DB/lock/disk/env-free): auth (SHA-256 key -> HashMap in snapshot) -> body read
  (bounded, streaming upload for large) -> parse only {model, stream, stream_options} -> budget reserve
  (DashMap AtomicU64 CAS, RAII rollback) -> concurrency guard -> route (least-load + fallback + circuit) ->
  proxy (shared client, header filtering) -> ledger try_record (bounded channel -> async writer; drops +
  rate-limited log when full).
- Control plane: poll task reloads snapshot; admin API reloads synchronously before 200/204. /healthz liveness;
  /readyz config freshness (503 if never loaded / reload errored / success stale > readiness_max_stale_ms).
- Ledger: LedgerSink -> LedgerWriter (Postgres batch + JSONL fallback + replay). usage_ledger UNIQUE request_id.

## File map

- src/main.rs (boot/poll/ledger writer), src/lib.rs (modules), src/contract.rs (AppState/ConfigSnapshot/types),
- src/auth.rs (hash_key/authorize), src/budget/mod.rs (RamBudgetStore reserve/commit CAS),
- src/route/mod.rs (RamBackendPool least-load + circuit), src/proxy/mod.rs (forward + SSE tap + usage),
- src/ledger/mod.rs (LedgerSink/LedgerWriter/record_drop), src/config/mod.rs (DbConfigLoader + resolve_backend_key),
- src/admin/mod.rs (AdminState + admin API + user_router /portal/me), src/handlers.rs (axum router + pipeline),
- src/metrics.rs, src/bin/mock_upstream.rs (llm-router-mock).
- static/index.html — THE PORTAL UI (OpenRouter-like, Admin + User login, charts, logs, route wizard).
- migrations/0001_init.sql + 0002_ledger_lag_timestamps.sql. scripts/ (bench_real.py, seed_defaults.sql, test_postgres.sh).
- start.sh (install/status/seed/smoke/gate/set-key). Defaults: portal :18080, admin key brightoIsGreat@2026, PG :55432.
- swarm/audits/ — live CODEX auditor <-> coder channel (read latest first).

## Data model (Postgres)

- backends (id, name, base_url, api_key_ref env:NAME|file:/path, weight, max_inflight 0=inf, format, enabled).
- model_routes (model_name PK, backend_ids JSON text, fallback_backend_id, chars_per_token, first_byte_timeout).
- teams (id, name, budget JSON text, enabled).
- api_keys (id, key_hash, key_prefix, team_id, owner, allowed_models JSON, budget, rpm_limit, concurrency_limit, expires_at, enabled).
- usage_ledger (ts, request_id UNIQUE, key_id, team_id, model, backend_id, status, input/output_tokens, estimated, ttfb_ms, total_ms, router_overhead_ms, stream, client_aborted, error_class, completed_at_ms, inserted_at_ms).

## API (admin requires x-admin-key + ADMIN_ALLOW_CIDR; user requires Bearer API key)

- Admin: GET /admin/backends, PATCH /admin/backends/{id}, GET /admin/backends/{id}/models, GET/POST /admin/routes,
  GET/POST /admin/teams, PATCH /admin/teams/{id}, GET/POST /admin/keys, DELETE /admin/keys/{id},
  GET /admin/stats?team&days, GET /admin/usage?team&key&from&to.
- User: GET /portal/me, GET /portal/me/usage, GET /portal/me/stats (auth by client API key).

## Boundaries (CODEX hard rules)

1. No Redis default. 2. No prompt logging / semantic cache. 3. Provider key plaintext never returned (only key_resolved + api_key_ref).
4. No DB in hot path. 5. No full JSON parse of large messages in hot path. 6. No heavy frontend build. 7. No lock across .await.

## Commands

    ./start.sh install && ./start.sh status      # first run (portal http://127.0.0.1:18080/)
    ./start.sh set-key openai sk-...             # set provider key into .env
    ./start.sh seed && ./start.sh smoke
    make build && make check                     # build + fmt/clippy
    make test                                    # temp Postgres unless DATABASE_URL/TEST_DATABASE_URL set
    cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings
    BASELINE_BOOTSTRAP=1 ./start.sh gate         # release benchmark (long)

## Gotchas

- axum 0.8 path params are {id}, not :id. sqlx QueryBuilder::Separated inserts ", " before bind -> manual first flag.
- DashMap entry() takes a WRITE lock -> hot path uses get() first, entry() only on miss.
- resolve_backend_key: env:NAME / file:/path / bare env name. DB stores only the reference.
- Ledger drop path stays cheap (rate-limited log + cached metrics::Counter handle).
- /readyz flips 503 on stale success, not only returned reload errors (reload can hang).

## Status (2026-09-17)

Portal rebuilt (OpenRouter-like) + self-service endpoints added. See swarm/audits/DEEPSEEK-FRONTEND-HANDOFF-20260917.md
and swarm/audits/DEEPSEEK-PORTAL-POLISH-DONE-20260917.md for handoff and acceptance checks.

