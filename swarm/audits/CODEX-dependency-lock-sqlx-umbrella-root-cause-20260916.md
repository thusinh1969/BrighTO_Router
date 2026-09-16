# Dependency audit — SQLite entries come from `sqlx` umbrella crate, not simple stale lockfile

Time: 2026-09-16 21:xx ICT  
Scope: current worktree. Codex did not edit dependency manifests.

## Verdict

Root production build currently does **not** include Redis. SQLite is more subtle:

- `cargo tree -i sqlx-sqlite` prints `warning: nothing to print`.
- `cargo tree -i libsqlite3-sys` prints `warning: nothing to print`.
- `cargo tree -i redis` errors because no Redis package exists.
- But `cargo metadata --locked` still resolves `sqlx-sqlite`, `sqlx-mysql`, and `libsqlite3-sys` because the root manifest depends on the `sqlx` umbrella crate.

So do **not** keep saying this is only a stale `Cargo.lock`. Regenerating the lockfile alone is unlikely to remove these entries as long as the dependency remains:

```toml
sqlx = { version = "0.9.0", default-features = false, features = ["runtime-tokio", "tls-rustls-aws-lc-rs", "postgres", "macros", "migrate"] }
```

## Evidence

```bash
cargo tree -i sqlx-sqlite
# warning: nothing to print.

cargo tree -i libsqlite3-sys
# warning: nothing to print.

cargo tree -i redis
# error: package ID specification `redis` did not match any packages
```

But `cargo metadata --locked` says:

```text
redis in_packages False in_resolve False
sqlx-sqlite in_packages True in_resolve True
libsqlite3-sys in_packages True in_resolve True
resolved_sqlx_family ['sqlx', 'sqlx-core', 'sqlx-macros', 'sqlx-macros-core', 'sqlx-mysql', 'sqlx-postgres', 'sqlx-sqlite']
```

And the resolve edge is from `sqlx` itself:

```text
parent sqlx 0.9.0 features ['_rt-tokio', 'derive', 'macros', 'migrate', 'postgres', 'runtime-tokio', 'sqlx-macros', 'sqlx-postgres', 'tls-rustls-aws-lc-rs']
 dep sqlx_sqlite -> sqlx-sqlite@0.9.0
 dep sqlx_mysql  -> sqlx-mysql@0.9.0
```

## Production meaning

This is not a hot-path runtime latency bug. Current gates prove the root binary builds and tests with Postgres-only behavior, and no Redis package is present.

It is still a supply-chain/packaging cleanliness issue if the project’s production contract is “Postgres only” and the lockfile must not contain unused database drivers.

## Exact fix options

### Option A — pragmatic production default

Keep `sqlx` umbrella dependency for now. Update docs/audit wording to be precise:

```text
Production runtime/build path is Postgres-only; Redis is absent. Cargo.lock still contains sqlx-mysql/sqlx-sqlite because sqlx umbrella 0.9 declares driver packages in its dependency graph.
```

This is acceptable if the immediate priority is performance/correctness, because replacing the umbrella crate churns many imports and may not affect runtime speed.

### Option B — strict lockfile purity

Replace `sqlx` umbrella usage with direct crates where possible:

- `sqlx-core`
- `sqlx-postgres`
- `sqlx-macros` / `sqlx-macros-core` only if needed by `#[sqlx::test]`/migrations

Then update imports/types throughout source so code no longer depends on `sqlx::...` re-exports. Validate whether `#[sqlx::test]` still has an ergonomic path; if not, keep umbrella in `dev-dependencies` only or replace sqlx test macro with explicit Postgres container setup.

Do not choose Option B casually. It is more invasive and should be justified by a strict supply-chain target, not latency. It is not needed to fix current P0/P1 behavior.

## Validation commands

For Option A:

```bash
cargo tree -i redis
cargo tree -i sqlx-sqlite
cargo tree -i libsqlite3-sys
cargo metadata --locked --format-version 1
```

For Option B, the success criterion is stricter:

```text
cargo metadata --locked: sqlx-sqlite/libsqlite3-sys/sqlx-mysql absent from resolve graph
cargo check/fmt/clippy/test/release all green
```

## Recommendation

Use Option A until benchmark and correctness blockers are closed. Chasing lockfile purity before the SOTA benchmark artifact is lower ROI and risks churn. If production policy later requires no SQLite/MySQL packages even in lockfile, do Option B as a dedicated dependency PR.
