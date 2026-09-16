# Directory tree cleanup verdict - 2026-09-17

## Current problem

The repo still looks mixed between production source, benchmark artifacts, and old swarm planning material.

Observed tree from current Git state:

```text
src/                 keep: Rust production source
tests/               keep: Rust integration tests and fixtures
migrations/          keep: PostgreSQL schema migrations
scripts/             keep, but only active install/test/benchmark scripts
benchmarks/          keep: benchmark docs/config/payload generator
bench/               remove or consolidate: legacy output/baseline location only
k8s/                 keep: optional production deployment starter
static/              keep: embedded Portal source
swarm/audits/        keep temporarily for Codex/DeepSeek collaboration
swarm/docs_builds/   remove before public release: old planning/mockup material
swarm/scripts/       keep only audit_watch.sh while collaboration is active; remove old swarm helper scripts before public release
target/              never commit: local Rust build cache, currently about 18GB
.ipynb_checkpoints/  never commit: local notebook artifact
```

## Concrete verdict

### Keep `migrations/`

Do not move it. This is the standard SQLx/PostgreSQL migration location and is referenced by:

```text
start.sh
scripts/test_postgres.sh
Dockerfile
scripts/bench_real.py
sqlx::test annotations
```

Moving it creates churn with no product gain.

### Keep `k8s/`

Keep it because the user explicitly wants a simple Kubernetes path. It is small and production-facing. It should stay as starter manifests only: deployment, service, config map, secret example, dev Postgres manifest, README.

### Keep `tests/`

Do not move it into `swarm/`. This is Rust convention and Cargo discovers integration tests there. `tests/fixtures/` is also referenced from production unit tests.

### Remove local `target/` only when no build/test is running

`target/` is ignored and not pushed to GitHub. Current local size is about 18GB. It can be deleted to save disk, but not while cargo/rustc is running.

### Consolidate `bench/` and `benchmarks/`

Current split is confusing:

```text
bench/README.md                tracked legacy explanation
bench/results/                 ignored generated results
benchmarks/README.md           real benchmark guide
benchmarks/BENCHMARK.md        release contract
benchmarks/STRATEGY.md         strategy
benchmarks/thresholds.toml     thresholds
benchmarks/make_payloads.py    payload generator
```

Recommended cleanup:

1. Move benchmark result output from `bench/results/<timestamp>/` to `benchmarks/results/<timestamp>/`.
2. Move baseline path from `bench/baseline.json` to `benchmarks/baseline.json`.
3. Update `scripts/bench_real.py`, `benchmarks/*.md`, `README.md`, and `Makefile` wording.
4. Delete tracked `bench/README.md` and the `bench/` folder.

This is documentation/harness cleanup only; it must not change benchmark method or thresholds.

### Shrink `swarm/` before public release

While Codex/DeepSeek collaboration is active, keep:

```text
swarm/audits/*.md
swarm/scripts/audit_watch.sh
swarm/README.md
```

Before public release, remove from tracked Git unless the user wants to expose agent history:

```text
swarm/docs_builds/
swarm/scripts/bench_smoke.py
swarm/scripts/capture_fixtures.py
swarm/scripts/stream_options_smoke.py
swarm/scripts/swarm_call.py
```

Reason: those files are old internal planning/test helpers. Some reference stale names such as `brigto-router`, old bench paths, and old agent ownership notes. They make the repo look unfinished.

## Safe cleanup order for DeepSeek

Do this after the current frontend/backend changes are committed or stashed, not in the middle of an active edit:

1. Finish and commit the Portal/frontend bundle.
2. Run `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `./scripts/test_postgres.sh`.
3. Consolidate `bench/` into `benchmarks/` as described above.
4. Remove stale `swarm/docs_builds/` and old `swarm/scripts/*` except `audit_watch.sh`.
5. Confirm `git ls-files` contains no generated artifacts, no `target/`, no `.ipynb_checkpoints`, no `swarm/out`, and no `WATCH.log`.
6. Rebuild/push Docker image only after the code tree is green.

## Public repo target tree

Target top-level tree for GitHub:

```text
.github/
benchmarks/
k8s/
migrations/
scripts/
src/
static/
tests/
.dockerignore
.env.example
.gitignore
BENCHMARK.md
Cargo.lock
Cargo.toml
CONTRIBUTING.md
Dockerfile
INSTALL.md
LICENSE
Makefile
PROVIDERS.md
README.md
SECURITY.md
start.sh
```

Temporary collaboration tree until release:

```text
swarm/audits/
swarm/scripts/audit_watch.sh
swarm/README.md
```
