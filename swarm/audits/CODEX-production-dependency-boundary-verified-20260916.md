# CODEX production dependency boundary — verified

Date: 2026-09-16 22:13 +07

Verdict: current dependency direction is mostly correct for fastest production profile on database/cache boundaries: PostgreSQL is present and Redis is not in default runtime dependencies. One dependency issue remains: `reqwest` still enables `gzip` until `CODEX-apply-no-gzip-identity-backend-20260916.patch` is applied. Do not reintroduce Redis or SQLite/Any into the router runtime while fixing latency.

## Evidence

`Cargo.toml` current state:

- `reqwest` uses `default-features = false` with rustls/json/stream/**gzip**. Gzip should be removed by `CODEX-apply-no-gzip-identity-backend-20260916.patch`.
- `sqlx` uses `default-features = false` with `runtime-tokio`, `tls-rustls-aws-lc-rs`, `postgres`, `macros`, `migrate`.
- Cargo comment says Redis is not in default profile.
- No `redis` dependency is present in `Cargo.toml`.

Search evidence:

```bash
rg -n 'Redis|redis|SQLite|sqlite|sqlx::Any|sqlx any|dev SQLite|prod Postgres|PostgreSQL ONLY|BUDGET_STORE|strict global' \
  Cargo.toml src docs benchmarks audits/README.md install docker-compose.yml
```

Current code evidence:

- `src/main.rs` opens Postgres pool at boot/config startup only.
- `src/config/mod.rs` uses `PgPool`/`PgRow`, not `AnyPool` or SQLite.
- `src/ledger/mod.rs` uses `PgPool` and background writer/replay; request path calls only `LedgerSink::try_record`.
- `src/admin/mod.rs` uses Postgres pool; admin is control plane, not inference hot path.
- `src/handlers.rs` and `src/proxy/mod.rs` have no `sqlx` query/fetch/execute calls.

## Stale/conflicting text to clean after code settles

These are not runtime blockers, but they confuse agents:

- `src/contract.rs:6` says `dev SQLite, prod Postgres`; current `Cargo.toml` says production PostgreSQL only and has no sqlite/any feature.
- `audits/README.md:20` still says dev DB SQLite/sqlx any.
- `docs/agents/a1_config.md`, `docs/agents/b2_ledger.md`, and `docs/agents/b3_admin_portal.md` still describe old SQLite/Any tasks.
- `docs/llm-router-rust-plan.md:187` still says `sqlx (postgres, sqlite cho dev)`.

Do not use those stale docs to justify adding SQLite/Any back into runtime. If docs are cleaned, make it comment/doc-only and keep production code on PostgreSQL.

## Production rule

- Reqwest gzip auto-decode: no in fastest profile; force backend `Accept-Encoding: identity`.
- PostgreSQL: yes, for config, admin persistence, ledger storage, and usage seed loading.
- PostgreSQL in inference request hot path: no.
- Redis: no in fastest profile.
- Redis future feature: only after a measured production requirement for strict global multi-node quota/concurrency.
- SQLite/Any: no in current production code path; do not add it for convenience if it weakens type clarity or test realism.

## Action for DeepSeek

When applying latency fixes, keep this boundary:

```text
request handler/proxy -> RAM snapshot + atomic budget + backend lease + reqwest client + LedgerSink.try_record
background/control plane -> PostgreSQL
optional future strict quota -> Redis feature only, separate from fastest profile
```

Any PR that adds Redis, SQL queries, filesystem reads, env reads, or new network calls inside `handle_generate` / `proxy_forward` must be rejected unless a benchmark/prod requirement proves it is necessary.
