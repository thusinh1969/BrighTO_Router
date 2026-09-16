# CODEX Postgres-only DB layer verdict — 2026-09-16

Scope: current worktree plus live Postgres probes. I did not edit `src/` or migrations.

## Verdict

CURRENT UPDATE: after this file was first written, DeepSeek changed `migrations/0001_init.sql` to Postgres DDL, and clean Postgres migration now applies. The DB-layer verdict still stands because current source still fails production boot on aggregate decode and still contains `sqlx::Any`/`?` placeholder risks. See `CODEX-current-production-smoke-red-20260916.md` for the latest live gate.

P0: stop trying to make the production DB layer dual-dialect through `sqlx::Any`. Use PostgreSQL as the production DB layer. Keep SQLite only for isolated unit tests if useful, not as the runtime config/admin/ledger database.

This is the least over-engineered production path. The current `sqlx::Any` approach has already produced four separate Postgres failures: migration syntax, boolean decode, aggregate decode, and bind placeholder syntax.

## Verified failures

### 1. Historical: root migration failed on clean Postgres before the latest migration patch

Current status: this specific DDL syntax failure has been fixed; clean Postgres migration now applies. The historical evidence below explains why the DB layer was audited.

Command shape used:

```bash
docker run --rm -d --name brigto_pg_mig_... \
  -e POSTGRES_DB=llm_router \
  -e POSTGRES_USER=llm_router \
  -e POSTGRES_PASSWORD=llm_router_dev \
  -p 127.0.0.1::5432 \
  pgvector/pgvector:pg16

DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router sqlx migrate run
```

Observed output:

```text
error: while executing migration 1: error returned from database: syntax error at or near "PRAGMA" at line 1244
```

Root cause:

- `migrations/0001_init.sql:3` is `PRAGMA foreign_keys = ON`.
- `migrations/0001_init.sql:6`, `:25`, `:32` use `INTEGER PRIMARY KEY AUTOINCREMENT`.

### 2. With Postgres BOOLEAN schema, router boot fails in config loader

I created a clean Postgres schema using `BOOLEAN` for enabled/estimated/stream/client_aborted and started `target/release/brigto-router`.

Observed output:

```text
Error: error occurred while decoding column 7: mismatched types; Rust type `i64` is not compatible with SQL type `BOOLEAN`

Caused by:
    mismatched types; Rust type `i64` is not compatible with SQL type `BOOLEAN`
```

Root cause:

- `src/config/mod.rs:79-87` reads `backends.enabled` as `i64`.
- `src/config/mod.rs:144-148` reads `teams.enabled` as `i64`.
- `src/config/mod.rs:171-181` reads `api_keys.enabled` as `i64`.

### 3. With Postgres BIGINT usage columns, router boot fails on aggregate decode

I created a Postgres schema with `usage_ledger.input_tokens/output_tokens BIGINT`. Router boot failed while loading ledger totals.

Observed output:

```text
Error: load usage_ledger boot counter

Caused by:
    0: error occurred while decoding column coalesce: error in Any driver mapping: Any driver does not support the Postgres type PgTypeInfo(Numeric)
    1: error in Any driver mapping: Any driver does not support the Postgres type PgTypeInfo(Numeric)
    2: Any driver does not support the Postgres type PgTypeInfo(Numeric)
```

Root cause:

- `src/config/mod.rs:53-54` uses `COALESCE(SUM(input_tokens), 0)` and decodes via `g_i64`.
- In Postgres, `SUM(BIGINT)` returns `NUMERIC`, which `sqlx::Any` does not map to `i64` here.
- `src/ledger/mod.rs:295-297` has the same risk for `SUM(input_tokens + output_tokens)`.

### 4. With integer schema compatible enough to boot, admin bind query fails on Postgres

I created a Postgres schema using integer flags and integer token columns so the current loader could boot. Then I started `target/release/brigto-router` and called:

```bash
curl -i -X POST http://127.0.0.1:18092/admin/teams \
  -H 'authorization: Bearer adminkey' \
  -H 'content-type: application/json' \
  --data '{"name":"newteam","enabled":true}'
```

Observed response:

```text
HTTP/1.1 500 Internal Server Error
error returned from database: syntax error at or near "," at line 1244
```

Root cause:

- `src/admin/mod.rs:328-329` uses `VALUES (?, ?, ?) RETURNING id`.
- `src/admin/mod.rs:369-372` uses `VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id`.
- `src/admin/mod.rs:445` uses `WHERE id = ?`.
- `src/ledger/mod.rs:275` also uses `request_id = ?`.
- SQLx docs in the installed source say Postgres and SQLite use `$1`, `$2`, `$3`; `?` is for MySQL/MariaDB.
- `QueryBuilder::<sqlx::Any>::push_bind` is also unsafe for Postgres here: `sqlx-core-0.9.0/src/arguments.rs` default `format_placeholder()` writes `?`, and `sqlx-core-0.9.0/src/any/arguments.rs` does not override it.

## Required architecture decision

Use PostgreSQL in production code:

- `main.rs`: use `sqlx::PgPool` or `Pool<Postgres>`.
- `config.rs`: `DbConfigLoader` should own a `PgPool`.
- `ledger.rs`: writer should connect a `PgPool`; `QueryBuilder<Postgres>` is OK.
- `admin.rs`: admin state should own a `PgPool`; use `$1`, `$2` placeholders or `QueryBuilder<Postgres>`.
- `Cargo.toml`: remove `sqlite` and `any` features from production dependencies unless tests still need them behind dev-only support.
- `migrations/0001_init.sql`: make it pure Postgres.

Do not keep dialect branching in production. The router goal is fastest/maintainable production, not “runs on every SQL dialect.” DB is not in the request forwarding path, so using Postgres-specific code costs zero hot-path latency and removes a large class of production bugs.

## Concrete Postgres schema shape

Keep JSON-ish config columns as `TEXT` initially. That avoids adding array/JSONB decode logic while preserving production correctness.

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

For loader boot logging, cast aggregates:

```sql
SELECT
  COUNT(*)::BIGINT,
  COALESCE(SUM(input_tokens), 0)::BIGINT,
  COALESCE(SUM(output_tokens), 0)::BIGINT
FROM usage_ledger
```

For budget seed aggregates, also cast sums to `BIGINT`.

## Query rewrite examples

Admin create team:

```rust
let row = sqlx::query(
    "INSERT INTO teams (name, budget, enabled) VALUES ($1, $2, $3) RETURNING id",
)
.bind(&payload.name)
.bind(budget_json)
.bind(payload.enabled)
.fetch_one(pool)
.await?;
```

Ledger replay existence check:

```rust
sqlx::query("SELECT COUNT(*)::BIGINT FROM usage_ledger WHERE request_id = $1")
    .bind(ev.request_id.as_str())
```

Ledger insert batch:

```rust
let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
    "INSERT INTO usage_ledger (...) "
);
qb.push_values(batch.iter(), |mut b, ev| { ... });
qb.build().execute(pool).await?;
```

Admin dynamic usage query:

```rust
let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
    "SELECT ... FROM usage_ledger WHERE TRUE"
);
```

## Acceptance

After patch, these must pass:

```bash
rg -n "sqlx::Any|AnyPool|AnyPoolOptions|QueryBuilder::<sqlx::Any>|sqlite|PRAGMA|AUTOINCREMENT|VALUES \(\?" src migrations Cargo.toml
```

Expected for production code/migrations: no hits, except intentionally isolated test helpers if kept outside production modules.

Run a clean Postgres production smoke:

```bash
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router sqlx migrate run
DATABASE_URL=postgres://llm_router:llm_router_dev@127.0.0.1:<port>/llm_router \
  ADMIN_MASTER_KEY=adminkey \
  ADMIN_ALLOW_CIDR=127.0.0.1/32 \
  LISTEN_ADDR=127.0.0.1:18092 \
  CONFIG_POLL_SECS=3600 \
  RUST_LOG=error \
  target/release/brigto-router
```

Then verify:

```bash
curl -fsS http://127.0.0.1:18092/healthz
curl -fsS http://127.0.0.1:18092/v1/models -H 'authorization: Bearer lc-dev0001'
curl -fsS -X POST http://127.0.0.1:18092/admin/teams \
  -H 'authorization: Bearer adminkey' \
  -H 'content-type: application/json' \
  --data '{"name":"newteam","enabled":true}'
```

And all Rust gates:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --release --locked
```

## What not to do

- Do not add Redis for any DB compatibility problem.
- Do not keep Postgres migration plus SQLite runtime assumptions.
- Do not introduce a generic repository abstraction layer. The DB is outside the hot path; typed Postgres queries are enough.
- Do not use JSONB or arrays until there is a production query requirement. JSON text is fine for config blobs loaded outside the request path.
