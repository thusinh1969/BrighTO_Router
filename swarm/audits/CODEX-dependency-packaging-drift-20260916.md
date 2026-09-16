# CODEX dependency/packaging drift audit — current root is lean, copies are stale

Time: 2026-09-16 Asia/Ho_Chi_Minh.
Scope: current worktree dependency surface after Postgres-only conversion.
Rule from `audits/README.md`: Codex does not edit `src/`; this is audit guidance.

## Verdict

Root runtime dependency direction is correct: Redis is not in the current dependency graph, and SQLite/`sqlx::Any` are not used by current source.

Do not turn this into a P0 coding distraction. The P0 blockers remain admin PATCH SQL and disabled-team auth bypass. Packaging cleanup is next after those are fixed.

## Evidence

`Cargo.toml` root:

```text
sqlx = features ["runtime-tokio", "tls-rustls-aws-lc-rs", "postgres", "macros", "migrate"]
# Redis KHÔNG có trong default profile
```

Dependency graph checks:

```text
cargo tree -i redis        -> package ID `redis` did not match any packages
cargo tree -i sqlx-sqlite  -> warning: nothing to print
cargo tree -i libsqlite3-sys -> warning: nothing to print
```

So `Cargo.lock` entries for `sqlx-sqlite` and `libsqlite3-sys` are stale lockfile noise, not active build dependencies.

## Drift still present

Current grep still finds stale Redis/Valkey/SQLite-era references in non-root or historical artifacts:

```text
Cargo.lock: stale sqlx-sqlite/libsqlite3-sys entries, not in cargo tree
scripts/fix_a1.py: stale AnyPool/sqlx::Any migration helper
install/README.md: Valkey/Redis text
install/README-setup.md: Valkey text
install/router-setup/README-setup.md: Valkey text
install/router-setup/Makefile: dev stack says valkey
benchmarks/router-setup/*: Valkey/Redis copy and broken jq |.*100
benchmarks/BENCHMARK.md: E4 redis_down gate no longer matches default production profile
```

`bench_smoke.py` has been updated to use real Postgres now. It is no longer the old SQLite smoke script, but it still depends on a live llama-server at `127.0.0.1:8088` and is only a directional smoke, not SOTA proof.

## Exact cleanup after P0 fixes

1. Remove stale Redis/Valkey wording from install docs and copied setup dirs, or clearly mark it as future optional strict multi-instance quota only.
2. Fix copied benchmark jq bugs:

```text
install/router-setup/bench/run.sh
benchmarks/router-setup/bench/run.sh
```

Replace `(.summary.requestsPerSec|.*100|round/100)` with the already-fixed root expression:

```jq
((.summary.requestsPerSec * 100 | round) / 100)
```

3. Delete or archive stale `scripts/fix_a1.py` if it is no longer used; it still encodes the old AnyPool direction and can mislead future agents.
4. Regenerate `Cargo.lock` only if the team wants a clean lockfile diff. Since `cargo tree` proves SQLite packages are not active dependencies, this is cleanup, not runtime risk.
5. Update `benchmarks/BENCHMARK.md` gate E4: default profile has no Redis. Redis outage gate only belongs to a future optional Redis feature.

## Do not do

- Do not re-add Redis to satisfy stale docs/benchmarks.
- Do not keep benchmark copies that test a different architecture than root.
- Do not claim production install is clean until root + install + benchmark setup all describe the same Postgres-only default profile.
