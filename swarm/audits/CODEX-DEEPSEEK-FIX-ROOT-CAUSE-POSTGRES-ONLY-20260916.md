# CODEX TO DEEPSEEK — FIX ROOT CAUSE 1 LẦN: Postgres-only DB layer

Time: 2026-09-16 19:21:16 +07. Scope: current moving worktree. I did not edit `src/`; this is an audit directive for DeepSeek.

## Verdict bắt buộc

Đừng sửa kiểu chắp vá. Root cause hiện tại là **đã tắt `sqlx` feature `any/sqlite` trong `Cargo.toml` nhưng code DB chưa được chuyển hết sang Postgres-only**.

Không được fix bằng cách bật lại `any`/`sqlite` trong root `Cargo.toml`. Làm vậy sẽ quay lại lỗi production đã verify: Postgres boot fail do `SUM(BIGINT)` -> `NUMERIC`, placeholder `?` trên admin/ledger, và dual-dialect workaround lan khắp code.

Fix đúng một lần: **toàn bộ production DB layer dùng `PgPool` + `sqlx::Postgres` + placeholder `$1/$2/...` + aggregate cast `::BIGINT`**.

## Current evidence

Sau patch mới nhất:

- `Cargo.toml` đã đúng hướng: root `sqlx` features chỉ còn `postgres`, `macros`, `migrate`; `redis` cũng đã bị remove khỏi dependency root.
- `src/config/mod.rs` đã bắt đầu đúng hướng: dùng `sqlx::postgres::{PgPool, PgRow}` và boot aggregate đã cast.
- Nhưng `cargo check --all-targets` vẫn FAIL vì các module còn lại vẫn gọi `sqlx::Any` trong khi feature `any` đã tắt.

Compile errors chính:

```text
error[E0432]: unresolved import `sqlx::AnyPool` --> src/admin/mod.rs:22:12
error[E0432]: unresolved import `sqlx::AnyPool` --> src/ledger/mod.rs:8:12
error[E0433]: cannot find `any` in `sqlx` --> src/admin/mod.rs:60:23
error[E0433]: cannot find `any` in `sqlx` --> src/ledger/mod.rs:172:11
error[E0425]: cannot find type `Any` in crate `sqlx` at admin QueryBuilder/query callsites
error[E0425]: cannot find type `Any` in crate `sqlx` at ledger QueryBuilder/query callsites
```

`src/config/mod.rs` tests also have a path issue:

```text
error: paths relative to the current file's directory are not currently supported
src/config/mod.rs:299:31 #[sqlx::test(migrations = "migrations")]
src/config/mod.rs:331:31 #[sqlx::test(migrations = "migrations")]
```

## Exact patch sequence

### 1. `src/main.rs`: stop installing/opening Any pool

Current blocker:

- `src/main.rs:55`: `sqlx::any::install_default_drivers()`
- `src/main.rs:56`: `sqlx::any::AnyPoolOptions::new()`

Patch:

```rust
use sqlx::postgres::PgPoolOptions;
```

Then:

```rust
let cfg_pool = PgPoolOptions::new()
    .max_connections(2)
    .connect(&db_url)
    .await
    .context("connect config DB")?;
```

Delete `install_default_drivers()`.

### 2. `src/admin/mod.rs`: convert all Any usage to Pg

Current blockers:

- `src/admin/mod.rs:22`: `use sqlx::{AnyPool, Row};`
- `src/admin/mod.rs:33`: `OnceCell<AnyPool>`
- `src/admin/mod.rs:57`: `Result<&AnyPool, ApiError>`
- `src/admin/mod.rs:60`: `sqlx::any::AnyPoolOptions::new()`
- `src/admin/mod.rs:328`: `sqlx::query::<sqlx::Any>(...)`
- `src/admin/mod.rs:369`: `sqlx::query::<sqlx::Any>(...)`
- `src/admin/mod.rs:402`: `QueryBuilder::<sqlx::Any>`
- `src/admin/mod.rs:445`: `sqlx::query::<sqlx::Any>(...)`
- `src/admin/mod.rs:466`: `QueryBuilder::<sqlx::Any>`

Patch pattern:

```rust
use sqlx::{PgPool, Row};
use sqlx::postgres::PgPoolOptions;
```

```rust
pool: Arc<tokio::sync::OnceCell<PgPool>>,
async fn pool(&self) -> Result<&PgPool, ApiError> { ... }
```

```rust
PgPoolOptions::new().max_connections(5).connect(&self.db_url).await
```

For raw query placeholders:

```sql
INSERT INTO teams (name, budget, enabled) VALUES ($1, $2, $3) RETURNING id
```

```sql
INSERT INTO api_keys (...) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id
```

```sql
UPDATE api_keys SET enabled = false WHERE id = $1
```

For dynamic queries:

```rust
sqlx::QueryBuilder::<sqlx::Postgres>::new(...)
```

Do not add a DB abstraction trait.

### 3. `src/ledger/mod.rs`: convert all Any usage to Pg

Current blockers:

- `src/ledger/mod.rs:8`: `use sqlx::{AnyPool, Row};`
- `src/ledger/mod.rs:79,154,171,184,194,243,268,299,399,417`: function signatures still use `AnyPool`.
- `src/ledger/mod.rs:172,177,400,410`: still calls `sqlx::any::*`.
- `src/ledger/mod.rs:199`: `QueryBuilder::<sqlx::Any>`.
- `src/ledger/mod.rs:279`: `WHERE request_id = ?`.
- `src/ledger/mod.rs:310/328/347/365`: usage seed queries use `sqlx::Any`, `?`, and uncast aggregate sums.
- `src/ledger/mod.rs:418,474`: tests still use `sqlx::Any`.

Patch pattern:

```rust
use sqlx::{PgPool, Row};
use sqlx::postgres::PgPoolOptions;
```

`connect_pool()`:

```rust
PgPoolOptions::new()
    .max_connections(2)
    .connect(&database_url)
    .await
```

Batch insert:

```rust
let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new("INSERT INTO usage_ledger ... ");
```

Replay exists query:

```sql
SELECT COUNT(*)::BIGINT FROM usage_ledger WHERE request_id = $1
```

Seed queries must cast:

```sql
SELECT key_id, COALESCE(SUM(input_tokens + output_tokens), 0)::BIGINT AS used
FROM usage_ledger
WHERE ts >= $1
GROUP BY key_id
```

Do the same for key/model, team total, team/model.

### 4. `tests/streaming_integration.rs`: stop using Any/SQLite for integration

Current blocker:

- `tests/streaming_integration.rs:64`: `sqlx::any::AnyPoolOptions::new()`

Best fix for production proof: use `#[sqlx::test(migrations = "./migrations")]` or a test helper that creates a temporary Postgres DB with migrations, then seed rows through `PgPool`.

If `#[sqlx::test]` path errors, use root-relative path exactly as sqlx expects for this version, or remove the macro and create a test Postgres pool from `DATABASE_URL` in a helper. Do not keep SQLite integration for production DB code after removing `sqlite` feature.

### 5. `src/config/mod.rs`: finish test macro path

Current source already mostly migrated, but `cargo check` reports:

```text
paths relative to the current file's directory are not currently supported
```

Fix the two macro attributes at `src/config/mod.rs:299` and `src/config/mod.rs:331`.

Likely shape:

```rust
#[sqlx::test(migrations = "./migrations")]
```

If this sqlx version still rejects it, use a manual PgPool test helper instead of fighting macro paths.

### 6. Then run gates in this exact order

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --no-fail-fast
cargo build --release --locked
```

### 7. Then prove production, not just compile

On a clean `pgvector/pgvector:pg16`:

1. `sqlx migrate run`
2. boot `target/release/brigto-router` until it listens
3. `GET /healthz`
4. admin create/list/update/disable
5. ledger insert through one non-stream request
6. streaming request with usage -> ledger non-zero, `estimated=false`
7. streaming request without usage -> ledger non-zero, `estimated=true`
8. restart router -> budget counters seeded from ledger

## What not to do

- Do not re-enable `sqlx` `any` or `sqlite` in root `Cargo.toml` just to make compile pass.
- Do not add Redis/Valkey for local budget/concurrency.
- Do not add DB calls to request hot path.
- Do not add traits/generic DB abstraction.
- Do not claim SOTA/fastest until `bench/run.sh` runs and saves real router-vs-direct results.

## Separate still-open issue: benchmark script

`bench/run.sh:19` still has invalid jq:

```bash
.summary.requestsPerSec|.*100|round/100
```

Exact fix:

```jq
((.summary.requestsPerSec * 100 | round) / 100)
```

Also extend benchmark to cover concurrency matrix `1 50 200`, not only one `CONC` per run, if this script is meant to be the official SOTA proof artifact.


## UPDATE 2026-09-16 19:21:40 +07 — after admin/ledger partial fix

DeepSeek moved `src/admin/mod.rs` and `src/ledger/mod.rs` to `PgPool`/`sqlx::Postgres`. Good. Do not rework those unless tests/prod smoke expose real runtime bugs.

Current `cargo check --all-targets` still fails, but the remaining compile root cause is now smaller:

```text
error: paths relative to the current file's directory are not currently supported
src/config/mod.rs:299:31 #[sqlx::test(migrations = "migrations")]
src/config/mod.rs:331:31 #[sqlx::test(migrations = "migrations")]
src/ledger/mod.rs:442:31 #[sqlx::test(migrations = "migrations")]
src/ledger/mod.rs:477:31 #[sqlx::test(migrations = "migrations")]

error[E0433]: cannot find `any` in `sqlx`
src/main.rs:55 sqlx::any::install_default_drivers()
src/main.rs:56 sqlx::any::AnyPoolOptions::new()
tests/streaming_integration.rs:52 sqlx::any::install_default_drivers()
tests/streaming_integration.rs:64 sqlx::any::AnyPoolOptions::new()
```

Exact remaining fix now:

1. `src/main.rs`: replace lines 55-58 with `PgPoolOptions::new().max_connections(2).connect(&db_url).await...`; delete `sqlx::any::install_default_drivers()`.
2. `src/config/mod.rs` and `src/ledger/mod.rs`: fix the four `#[sqlx::test(migrations = "migrations")]` attributes. If this sqlx version rejects relative paths there, replace macro tests with a manual Postgres test helper using `DATABASE_URL`; do not re-enable SQLite/Any.
3. `tests/streaming_integration.rs`: replace SQLite/Any helper with Postgres/PgPool helper. Best: `#[sqlx::test(migrations = "...")]` if accepted; otherwise manual temporary DB/schema through Postgres. This integration test should exercise production DB dialect now.
4. Keep root `Cargo.toml` Postgres-only. Do not re-add `any` or `sqlite`.
5. After compile passes, run full gate and clean Postgres boot/admin/ledger smoke.

Still-open non-DB issue: `bench/run.sh:19` jq expression remains invalid because of `|.*100`. Replace it with `((.summary.requestsPerSec * 100 | round) / 100)`.


## UPDATE 2026-09-16 19:22:31 +07 — sqlx test macro root cause confirmed

Current compile blockers after `main`, `admin`, `ledger`, and streaming integration moved toward Pg:

```text
paths relative to the current file's directory are not currently supported
```

Remaining attrs found by `rg`:

- `tests/streaming_integration.rs:124`: `#[sqlx::test(migrations = "migrations")]`
- `tests/streaming_integration.rs:169`: `#[sqlx::test(migrations = "migrations")]`
- `src/config/mod.rs:299`: `#[sqlx::test(migrations = "migrations")]`
- `src/config/mod.rs:331`: `#[sqlx::test(migrations = "migrations")]`
- `src/ledger/mod.rs:442`: `#[sqlx::test(migrations = "migrations")]`
- `src/ledger/mod.rs:477`: `#[sqlx::test(migrations = "migrations")]`

I checked the local sqlx 0.9.0 macro source, not guessed:

- `sqlx-macros-core-0.9.0/src/common.rs:16-24`: a relative path with no parent component is rejected with exactly this error.
- `sqlx-macros-core-0.9.0/src/common.rs:27-30`: accepted paths are resolved under `CARGO_MANIFEST_DIR`.
- `sqlx-macros-core-0.9.0/src/migrate.rs:9`: default path is `./migrations`.

Exact fix: change all six attributes to one of these two forms:

```rust
#[sqlx::test]
```

or:

```rust
#[sqlx::test(migrations = "./migrations")]
```

Prefer `#[sqlx::test]` because the crate already has root `./migrations` and the macro infers it when the test takes a `PgPool` input. Do not use `migrations = "migrations"`. Do not re-enable `sqlx any/sqlite` to bypass this.

After this patch, run:

```bash
cargo check --all-targets
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets --no-fail-fast
```

If tests then require a Postgres server through `DATABASE_URL`, that is expected for Postgres-only production DB tests; run them against clean `pgvector/pgvector:pg16` rather than restoring SQLite.
