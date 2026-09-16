# CODEX CURRENT — local gate green, Postgres production boot red

Time: 2026-09-16 19:17:55 +07. Scope: current worktree. I did not edit `src/`; audit handoff only.

## Verdict

This snapshot is **local-test green but production-red**. Do not claim production readiness or SOTA fastest yet.

What is now fixed and should not be churned:

- Rust local gate is green.
- Route fallback has concrete tests and passes.
- Streaming integration test isolation was fixed with `DB_COUNTER`; both streaming tests pass in parallel.
- Root `docker-compose.yml` no longer runs Valkey.
- Release build succeeds.
- Clean Postgres migration succeeds.

The remaining production blocker is narrow but architectural: DB code still uses `sqlx::Any` for a Postgres-only production system. That choice is already breaking release boot on clean Postgres and will also keep admin/ledger paths fragile because placeholders/types differ by dialect.

## Evidence: local Rust gate is green

Command run:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --no-fail-fast
```

Result:

- `cargo check --all-targets`: PASS.
- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --all-targets -- -D warnings`: PASS.
- `cargo test --all-targets --no-fail-fast`: PASS.
- lib tests: 38/38 PASS.
- `tests/streaming_integration.rs`: 2/2 PASS.

Streaming isolation proof:

```bash
cargo test --test streaming_integration -- --nocapture --test-threads=1
cargo test --test streaming_integration -- --nocapture
```

Both pass. `tests/streaming_integration.rs:23` now has `DB_COUNTER`, and `tests/streaming_integration.rs:53-60` includes that counter in the SQLite temp DB name.

## Evidence: production Postgres boot is still red

Procedure run on current snapshot:

1. `cargo build --release --locked`: PASS.
2. Start fresh `pgvector/pgvector:pg16`.
3. `DATABASE_URL=postgres://... sqlx migrate run`: PASS.
4. Boot `target/release/brigto-router` against that DB.

Boot result:

```text
Error: load usage_ledger boot counter

Caused by:
    0: error occurred while decoding column coalesce: error in Any driver mapping: Any driver does not support the Postgres type PgTypeInfo(Numeric)
    1: error in Any driver mapping: Any driver does not support the Postgres type PgTypeInfo(Numeric)
    2: Any driver does not support the Postgres type PgTypeInfo(Numeric)
```

Direct failing code:

- `src/config/mod.rs:53-55`:

```sql
SELECT COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0) FROM usage_ledger
```

- `src/config/mod.rs:59-63` decodes those columns as `i64`.

Why it fails: in Postgres, `SUM(BIGINT)` returns `NUMERIC`; `sqlx::Any` cannot decode that Postgres numeric as `i64`.

Immediate boot fix if DeepSeek wants a small tactical patch first:

```sql
SELECT
  COUNT(*)::BIGINT,
  COALESCE(SUM(input_tokens), 0)::BIGINT,
  COALESCE(SUM(output_tokens), 0)::BIGINT
FROM usage_ledger
```

That only fixes boot. It does not fix the DB architecture problem below.

## Required architecture fix: production DB must be Postgres-only

Keep the fastest runtime architecture simple:

- Request hot path: RAM config snapshot, RAM budget/concurrency, RAM backend lease, shared `reqwest::Client`, streaming tap. No DB/Redis/filesystem/env/full JSON parse/per-request client.
- Background/control plane: Postgres for config/admin/ledger.
- Redis/Valkey: absent from default fastest profile; optional future feature only if strict global quota across router instances is proven necessary by production requirements and benchmark data.

Current code contradicting Postgres-only production:

- `Cargo.toml:39` still enables `postgres`, `sqlite`, and `any` sqlx features together.
- `src/main.rs:55-58` installs Any drivers and opens config DB with `AnyPoolOptions`.
- `src/config/mod.rs:6-7` imports `AnyRow`, `Any`, and `Pool`; loader is still generic Any.
- `src/admin/mod.rs:22,33,57-64` uses `AnyPool` and `AnyPoolOptions`.
- `src/admin/mod.rs:328-333`, `369-383`, `445-447` use raw `?` placeholders in queries that must run on Postgres.
- `src/admin/mod.rs:402` and `466` use `QueryBuilder::<sqlx::Any>`.
- `src/ledger/mod.rs:8,79,171-184,194-223,268-285,299-374` uses `AnyPool`, `QueryBuilder::<sqlx::Any>`, raw `?`, and Postgres-unsafe aggregate decode.

One-pass fix plan, without over-engineering:

1. Replace production DB pool types with `sqlx::PgPool`.
2. Replace pool creation with `sqlx::postgres::PgPoolOptions`.
3. Replace `QueryBuilder::<sqlx::Any>` with `QueryBuilder::<sqlx::Postgres>`.
4. Replace raw `?` placeholders with `$1`, `$2`, ... in raw Postgres queries.
5. Cast every aggregate read as `i64`:
   - `COALESCE(SUM(input_tokens), 0)::BIGINT`
   - `COALESCE(SUM(output_tokens), 0)::BIGINT`
   - `COALESCE(SUM(input_tokens + output_tokens), 0)::BIGINT`
6. Remove `sqlite` and `any` from default root `sqlx` features after DB tests no longer depend on production Any code.
7. Do not add a database abstraction trait. The repo needs one production dialect, not a generic DB framework.

## Benchmark artifact: file exists, script still red

DeepSeek added `bench/run.sh` and `bench/make_payloads.py`. Good direction; previous “missing file” blocker is gone. Current script still cannot be trusted as SOTA evidence.

Syntax checks:

```bash
bash -n bench/run.sh
python3 -m py_compile bench/make_payloads.py
```

Both pass.

Runtime jq check using sample oha JSON fails on `bench/run.sh:19`:

```text
jq: error: syntax error, unexpected INVALID_CHARACTER
```

Current line:

```bash
jq -r '"  rps \(.summary.requestsPerSec|.*100|round/100) ...'
```

Root cause: `|.*100` is invalid jq.

Exact fix for rps expression:

```jq
((.summary.requestsPerSec * 100 | round) / 100)
```

Also, `bench/run.sh:8` currently uses one `CONC` value per run. The SOTA proof needs a matrix. Either make `CONCS="1 50 200"` and loop over it, or document that CI/operator must run the script three times and include all result directories. The better artifact is to loop in the script and emit one summary table.

Minimum benchmark DoD before “fastest/SOTA” wording:

- direct vs router, same backend and same network class;
- payloads: 1K, 50K, 200K;
- concurrency: 1, 50, 200;
- streaming and non-streaming;
- report p50/p95/p99, rps, non-200, TTFB delta, and router-overhead delta;
- save raw oha/curl outputs under `bench/results/<timestamp>/`.

## Redis/Valkey cleanup still incomplete

Root `docker-compose.yml` is fixed. Install copies are stale and can mislead production users into deploying Redis by default:

- `install/docker-compose.yml:1,9,17,35-44` still has Valkey and `REDIS_URL`.
- `install/router-setup/docker-compose.yml:9,17,35-40` still has Valkey and `REDIS_URL`.
- `install/README.md:70,99,101` still tells production users to configure Redis/Valkey.
- `install/router-setup/Cargo.toml:34` and `install/Cargo.toml:34` still include `redis` dependency copies.
- `install/router-setup/Makefile:46` still says dev stack includes Valkey.

Fix: align install artifacts with root fastest profile: router + Postgres only. Add one short optional note for future Redis/Valkey if strict global quota is required.

## Next exact patch order

1. Fix Postgres boot in `src/config/mod.rs:53-55` by casting aggregates, unless doing full `PgPool` conversion immediately.
2. Convert config/admin/ledger/main DB code from `sqlx::Any` to Postgres-only `PgPool`/`Postgres` QueryBuilder.
3. Fix `bench/run.sh:19` jq expression.
4. Extend `bench/run.sh` to cover concurrency matrix `1 50 200` or emit a hard error if `CONC` is not one of those and document multi-run proof.
5. Clean install Redis/Valkey stale files.
6. Re-run local green gate.
7. Run clean Postgres smoke through boot, `/healthz`, admin create/list/update/disable, ledger insert/replay, stream with usage, stream without usage, and restart budget seed.
8. Only after that, run benchmark and compare router vs direct overhead.
