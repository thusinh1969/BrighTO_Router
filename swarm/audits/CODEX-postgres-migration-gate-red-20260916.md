# CODEX Postgres migration gate RED — 2026-09-16

Scope: current worktree. I did not edit `src/` or migration code; this file records a verified production gate failure.

## Verdict

P0: production Postgres migration is currently broken. Do not claim production-ready or SOTA until this passes on a clean Postgres database.

This is not theoretical. I ran the migration against a fresh `pgvector/pgvector:pg16` container and it failed immediately.

## Reproduction command used

```bash
c="brigto_pg_mig_$$"
docker run --rm -d --name "$c" \
  -e POSTGRES_DB=llm_router \
  -e POSTGRES_USER=llm_router \
  -e POSTGRES_PASSWORD=llm_router_dev \
  -p 127.0.0.1::5432 \
  pgvector/pgvector:pg16

port=$(docker port "$c" 5432/tcp | sed -E 's/.*:([0-9]+)$/\1/' | tail -1)
# wait until pg_isready passes, then:
DATABASE_URL="postgres://llm_router:llm_router_dev@127.0.0.1:${port}/llm_router" sqlx migrate run

docker rm -f "$c"
```

Observed output:

```text
error: while executing migration 1: error returned from database: syntax error at or near "PRAGMA" at line 1244
```

## Root cause

`migrations/0001_init.sql` is SQLite DDL, but root `.env.example` and `docker-compose.yml` use Postgres:

- `migrations/0001_init.sql:3` has `PRAGMA foreign_keys = ON`.
- `migrations/0001_init.sql:6`, `:25`, `:32` use `INTEGER PRIMARY KEY AUTOINCREMENT`.
- `.env.example:2` sets `DATABASE_URL=postgres://...`.
- `docker-compose.yml:8` sets `DATABASE_URL=postgres://...`.

## Fix once, no over-engineering

Use Postgres as the production/root migration dialect. Keep current JSON-shaped columns as `TEXT` for now so loader/admin remain simple and `sqlx::Any` does not need array/JSONB branching.

Replace SQLite-specific DDL with this shape:

```sql
CREATE TABLE IF NOT EXISTS backends (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    api_key_ref TEXT NOT NULL,
    weight INTEGER NOT NULL DEFAULT 1 CHECK (weight >= 1),
    max_inflight INTEGER NOT NULL DEFAULT 0,
    format TEXT NOT NULL CHECK (format IN ('openai','anthropic')),
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS model_routes (
    model_name TEXT PRIMARY KEY,
    backend_ids TEXT NOT NULL DEFAULT '[]',
    fallback_backend_id BIGINT,
    chars_per_token DOUBLE PRECISION NOT NULL DEFAULT 4.0,
    first_byte_timeout BIGINT NOT NULL DEFAULT 180
);

CREATE TABLE IF NOT EXISTS teams (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    budget TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS api_keys (
    id BIGSERIAL PRIMARY KEY,
    key_hash TEXT NOT NULL,
    key_prefix TEXT NOT NULL,
    team_id BIGINT NOT NULL REFERENCES teams(id),
    owner TEXT NOT NULL,
    allowed_models TEXT NOT NULL DEFAULT '[]',
    budget TEXT,
    rpm_limit INTEGER,
    concurrency_limit INTEGER,
    expires_at BIGINT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE IF NOT EXISTS usage_ledger (
    ts BIGINT NOT NULL,
    request_id TEXT NOT NULL,
    key_id BIGINT NOT NULL,
    team_id BIGINT NOT NULL,
    model TEXT NOT NULL,
    backend_id BIGINT NOT NULL,
    status INTEGER NOT NULL,
    input_tokens BIGINT NOT NULL,
    output_tokens BIGINT NOT NULL,
    estimated BOOLEAN NOT NULL,
    ttfb_ms BIGINT NOT NULL,
    total_ms BIGINT NOT NULL,
    router_overhead_ms BIGINT NOT NULL,
    stream BOOLEAN NOT NULL,
    client_aborted BOOLEAN NOT NULL,
    error_class TEXT
);

CREATE INDEX IF NOT EXISTS idx_usage_team_ts ON usage_ledger(team_id, ts);
CREATE INDEX IF NOT EXISTS idx_usage_key_ts ON usage_ledger(key_id, ts);
CREATE UNIQUE INDEX IF NOT EXISTS idx_usage_request_id ON usage_ledger(request_id);
```

Required code follow-up:

- Add `g_bool` in `src/config/mod.rs` using `row.try_get::<bool, _>(idx)`.
- Use `g_bool` for `backends.enabled`, `teams.enabled`, and `api_keys.enabled`.
- Keep existing SQLite inline test schemas separate for unit tests if needed. The production migration must be Postgres.

## Acceptance

Run this exact production gate on a clean Postgres DB:

```bash
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router sqlx migrate run
```

Then run:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release --locked
```

Expected: all pass.
