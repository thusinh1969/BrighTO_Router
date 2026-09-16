# CODEX audit — install/provider/README root-cause pass — 2026-09-17

## Verdict

The repo was close technically, but the public first-run experience still had root-cause gaps that would make the project look unfinished on GitHub:

1. README mixed quick-start material with SQL setup details.
2. Default local install used common ports `5432` and `8080`, which collided with existing host services in this audit environment.
3. Default provider records were not clearly documented and not exposed in a friendly portal flow.
4. Admin provider setup needed API endpoints for list/update/fetch-models/create-route.
5. Benchmark docs used phrases such as `offered rate` without a plain-language legend.
6. Portal HTML had markdown fences and a JavaScript brace error, so the portal could render or behave incorrectly.

## Fixes applied

- Rewrote `README.md` as a clean public landing page: pull, install, run, default seed, feature set, Rust rationale, benchmark summary, enterprise direction, and links to detail docs.
- Added `INSTALL.md` for local Docker, external PostgreSQL, Kubernetes, operations, and health checks.
- Added `PROVIDERS.md` for seeded provider templates and portal setup flow.
- Added root `BENCHMARK.md` as a simple pointer to the benchmark guide, contract, and strategy.
- Added `scripts/seed_defaults.sql`, safe to rerun.
- Seed now creates:
  - `Default Team`, enabled.
  - 10 provider templates, disabled by default: OpenAI, Anthropic, Gemini, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Meta Muse, Custom OpenAI-compatible.
  - no default client API keys.
  - no model routes until the admin chooses a model.
- Updated `.env.example`:
  - `ADMIN_MASTER_KEY=brightoIsGreat@2026`.
  - default local DB moved to `127.0.0.1:55432`.
  - default local router moved to `0.0.0.0:18080`.
  - provider API key env vars added.
- Updated `docker-compose.yml`:
  - image remains `thusinh1969/brighto_airouter:v1` with `pull_policy: missing`.
  - PostgreSQL host port is configurable and defaults to `55432`.
  - router no longer depends on local PostgreSQL so external DB mode can skip the local DB container.
- Updated `start.sh`:
  - `install`, `start`, `restart` run migrations and seed defaults.
  - `install --database-url URL` uses an existing PostgreSQL database and skips local PostgreSQL.
  - `install --k8s --replicas N` applies Kubernetes manifests, runs migrations/seed, and sets the right cluster DB URL when using the dev Postgres manifest.
  - `set-key <provider> <api-key>` writes provider keys to `.env` and recreates router if running.
  - old local defaults are upgraded only when they exactly match the previous defaults.
- Updated admin API:
  - `GET /admin/backends` lists backends without exposing provider keys.
  - `PATCH /admin/backends/{id}` updates provider templates and reloads config immediately.
  - `GET /admin/backends/{id}/models` fetches OpenAI-style model lists where supported.
  - `POST /admin/routes` creates or updates a model route and reloads config immediately.
- Updated proxy URL building to support both host-only provider base URLs and SDK-style base URLs with `/v1` prefix.
- Updated portal:
  - English UI for public repo.
  - provider list/edit/fetch-models/create-route flow.
  - no markdown fence in served HTML.
  - JavaScript syntax validated.
- Updated benchmark docs:
  - clear legend for offered rate, RPS, p50, p99, TTFB, RSS, and gate names.
  - explicit statement that offered rate is not a global standard; same-hardware direct-versus-router comparison is the defensible method.

## Evidence

Commands passed:

```bash
python3 -m py_compile scripts/bench_real.py benchmarks/make_payloads.py scripts/hotpath_guard.py
bash -n start.sh scripts/test_postgres.sh benchmarks/gate.sh
python3 scripts/hotpath_guard.py
node --check /tmp/brighto_portal.js
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo check --locked --all-targets
CARGO_INCREMENTAL=0 cargo clippy --locked --all-targets -- -D warnings
./scripts/test_postgres.sh
make test
DATABASE_URL=postgres://bad:bad@127.0.0.1:1/bad ./scripts/test_postgres.sh
./start.sh seed
./start.sh start
./start.sh status
./start.sh smoke
docker compose config
DOCKER_BUILDKIT=1 docker build -t thusinh1969/brighto_airouter:v1 .
```

Observed runtime evidence:

```text
default_team_rows=1
provider_template_rows=10
enabled_provider_templates=0
model_route_rows=0
api_key_rows=0
health=ok
ready=ready
admin_backend_rows=10
expected_provider_templates_present=true
```

Smoke benchmark evidence from `bench/results/20260917-025033`:

```text
1k c=50 overhead p50 +0.287ms p99 +0.418ms
50k c=50 overhead p50 +0.544ms p99 +0.707ms
200k c=50 overhead p50 +0.901ms p99 +0.958ms
B10 ledger lag p99 0.492050s rows 200/200 rps 200.57 non200 0
smoke command pass: True
release thresholds pass: True (not enforced in smoke)
```

## Remaining release work

- Run the full release gate after this commit because the Git SHA changed.
- Run the full 500k and 1M stress proof on the intended dual-Xeon server before publishing claims for those sizes.
- Do not claim “fastest in the world” until same-machine comparisons against named routers exist.

## Follow-up hardening — safe env parsing and isolated test DB

Additional audit after publishing found two more first-run risks:

1. `start.sh` sourced `.env` directly. That is unsafe for API-key files because shell metacharacters could be executed. Fixed by replacing shell sourcing with a tiny key-value parser that reads only the variables the script needs.
2. `scripts/test_postgres.sh` still had the old default URL on port `5432`, and `make test` could use a running local development DB exported from `.env`. Fixed by moving the default to `55432` and making default tests always use a temporary PostgreSQL container. A real DB is used only when `DATABASE_URL` or `TEST_DATABASE_URL` is set to a non-default URL explicitly.

Follow-up evidence:

```bash
bash -n start.sh scripts/test_postgres.sh benchmarks/gate.sh
python3 scripts/hotpath_guard.py
./start.sh status
make test
DATABASE_URL=postgres://bad:bad@127.0.0.1:1/bad ./scripts/test_postgres.sh
docker compose config
```

Observed:

```text
dotenv_safe_parser_ok=true
make test started temporary Postgres: brighto_test_pg_3065631
51 unit tests passed
4 integration tests passed
bad external DATABASE_URL failed as expected
healthz: ok
readyz: ready
```
