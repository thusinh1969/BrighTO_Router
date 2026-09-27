# Script catalog

This folder contains developer and release-support scripts. End users normally need only `./start.sh`, `./install.sh`, and `./test_router.py` from the repository root.

Keep scripts here small, explicit, and single-purpose. Put user-facing smoke tests under `smoke/<feature>/` when they create real routes or require external services.

## User-facing helpers

| Script | Purpose | Typical command |
|---|---|---|
| `rebuild_docker_local.sh` | Rebuild the Rust release binary, build a local Docker image, update `.env` to use it with pull policy `never`, and restart the router. | `./scripts/rebuild_docker_local.sh my-brighto-router:dev` |
| `tls_smoke.sh` | Check HTTP/HTTPS router health and TLS behavior. | `./scripts/tls_smoke.sh` |
| `test_postgres.sh` | Run PostgreSQL-backed integration checks. | `./scripts/test_postgres.sh` |

## Portal and API audit

These scripts are release gates for Portal/admin/API behavior. They should be run after UI, admin API, model route, provider key, Model Group, or install changes.

| Script | Purpose |
|---|---|
| `portal_browser_smoke.py` | Fast Playwright smoke for the real Portal flow and exact Admin API payloads. |
| `portal_full_audit.py` | Broader Portal audit covering screens, create/edit/delete flows, route/group behavior, and edge cases. |
| `portal_smoke.py` | Older/basic Portal smoke retained for compatibility with existing local workflows. |
| `api_matrix_smoke.py` | Deterministic mock coverage for supported API shapes. |
| `hotpath_guard.py` | Guards large-payload hot-path behavior against accidental full-body parsing. |

## Live provider smoke

These scripts make tiny live calls when provider keys are present. They are not stress tests.

| Script | Purpose |
|---|---|
| `adapter_smoke.py` | Direct provider checks for embeddings, rerank, and ASR adapters. |
| `adapter_router_smoke.py` | Calls providers through BrighTO-Router when configured routes exist. |
| `anthropic_smoke.py` | Live Anthropic Messages smoke when `ANTHROPIC_API_KEY` is present. |
| `real_provider_smoke.py` | Small end-to-end provider checks used during release validation. |

## Benchmark support

Router benchmarks live under `benchmarks/`. These support scripts are kept here because they are Python drivers used by release gates.

| Script | Purpose |
|---|---|
| `bench_full_stress.py` | Larger stress driver for throughput/latency measurement. |
| `bench_real.py` | Real-backend benchmark helper for controlled local tests. |
| `live_lb_real_test.py` | Live Model Group load-balancing check when configured providers are available. |

## Data seed

| File | Purpose |
|---|---|
| `seed_defaults.sql` | SQL seed helper retained for local/reset workflows; normal installs use `./start.sh seed` and admin APIs. |

## Where feature smoke tests belong

Feature smoke tests that create their own data or start an external service belong under `smoke/<feature>/`, not directly in `scripts/`.

Current examples:

- `smoke/model_group/` — mock and live Model Group load-balancing smoke.
- `smoke/systemone/` — Ollaya/Laya System One smoke with no-auth and Bearer-auth modes.
