# Contributing to BrighTO-Router

BrighTO-Router is benchmark-first infrastructure. Changes that touch routing, proxying, auth, budget, ledger, config reload, metrics, or startup behavior must preserve correctness and must not add work to the inference hot path without measurement.

## Development checks

Run the standard local checks before opening a pull request:

```bash
python3 scripts/hotpath_guard.py
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
make test
```

Integration tests use PostgreSQL through `sqlx::test`. Run `make test` for the safest local path; it starts a temporary PostgreSQL container unless you explicitly set a non-default `DATABASE_URL` or `TEST_DATABASE_URL`.

Portal, Admin API, provider-key, team, API-key, Docker/runtime, or install-flow changes must also run the real browser audit:

```bash
python3 scripts/portal_full_audit.py
```

For quick iteration after release binaries already exist, use:

```bash
BRIGHTO_SKIP_RELEASE_BUILD=1 python3 scripts/portal_full_audit.py
```

The audit starts temporary PostgreSQL, a local router, the Rust mock upstream, and headless Chromium. It clicks the main Portal sections and tests model-route, provider-key, team, and API-key flows end to end.

## Benchmark changes

The benchmark contract lives in `benchmarks/BENCHMARK.md`; thresholds live in `benchmarks/thresholds.toml`.

For performance-sensitive changes, attach the generated artifact directory from:

```bash
make bench-gate
```

A benchmark claim must include raw direct and router results, not only a summary. Public claims must compare the same hardware, payload, backend, TLS/proxy setup, and offered-rate shape.

## Architecture rules

- Keep the request path free of Postgres, Redis, filesystem, environment reads, and unbounded buffering.
- Keep control-plane and ledger work in background tasks.
- Add Redis or another shared store only behind an explicit feature/production mode with benchmark evidence.
- Do not forward client API keys to upstream providers.
- Do not retry after the first byte has been sent to the client.
- Keep streaming semantics intact; do not buffer streams to make benchmarks look better.

## Pull request expectations

A good PR states:

- What changed.
- Why the change is needed.
- Which tests or benchmark gates prove it.
- Any remaining gate or production limitation.

Small, reviewable slices are preferred. Avoid broad rewrites unless a measured root cause requires them.
