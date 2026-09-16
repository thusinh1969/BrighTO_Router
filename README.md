# BrighTO-Router

GitHub repository: `https://github.com/thusinh1969/Brighto_AIRouter`

BrighTO-Router is a high-performance LLM router written in Rust. LLM means Large Language Model. It sits between applications and model backends, keeps the API surface compatible with OpenAI-style clients, and adds the operational controls teams need in production: model routing, fallback, team/API-key budgets, rate limits, concurrency limits, usage ledger, admin portal, health checks, and Prometheus metrics. API means Application Programming Interface.

The project goal is explicit: keep the inference path as small and predictable as possible. The router should add sub-millisecond overhead in the common case, preserve streaming behavior, and avoid infrastructure that is not required for the fastest production profile.

Current status: this repository contains the full benchmark strategy, 500k/1M token-class stress payload support, router RSS memory sampling, baseline regression enforcement, and true B10 ledger-lag measurement. RSS means resident set size, the physical memory reported by Linux. B10 measures the p99 time from request completion in the router to insertion in PostgreSQL. The latest local checks verify Rust tests, Postgres integration, Docker Compose config, Docker Hub image availability, and a short B10 smoke run. A full public release baseline is still intentionally separate: run the full gate with `BASELINE_BOOTSTRAP=1`, review the result, then commit `bench/baseline.json` before claiming the entire `BENCHMARK.md` contract green for a release.

Common terms used in this README:

- **SDK** means Software Development Kit, usually a client library used by an application.
- **TTFB** means time to first byte, the time until the first response byte arrives.
- **RPS** means requests per second.
- **SME** means small or medium-sized enterprise.
- **JSONL** means newline-delimited JSON, one JSON record per line.
- **TLS** means Transport Layer Security, the encryption layer normally used by HTTPS.

## What it does

BrighTO-Router provides:

- OpenAI-compatible routing for `/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, and `/v1/models`.
- Anthropic-compatible passthrough for `/v1/messages`.
- Model aliases mapped to one or more backends.
- Weighted backend selection with inflight-aware routing.
- Optional declared fallback backend per model.
- Circuit breaking after repeated backend failures.
- API keys with SHA-256 hashes, prefixes, expiry, enabled/disabled state, and allowed model lists.
- Team and key budgets with per-model budget support.
- RPM and concurrency limits.
- Usage accounting from upstream `usage` fields, with safe estimation when upstream usage is missing.
- Async Postgres-backed usage ledger with file fallback if the database is temporarily unavailable.
- Admin API and embedded portal for teams, keys, budgets, and usage lookup.
- `/healthz`, `/readyz`, `/metrics`, and JSON logs.

It is designed for small and medium-sized enterprises and teams that need one internal LLM endpoint instead of many provider-specific endpoints and keys.

## Why Rust

LLM routing is latency-sensitive infrastructure. Python is excellent for orchestration, experiments, and data work, but it is the wrong default for a router whose job is to forward bytes, preserve streaming, maintain counters, and stay predictable under high concurrency.

Rust gives this project practical advantages:

- **Low tail overhead**: no interpreter, no GIL, no per-request runtime surprises.
- **Safe concurrency**: Tokio tasks, bounded channels, and ownership make it easier to keep background work off the request path.
- **Memory control**: large prompts and long streams can be passed through without accidental unbounded buffering.
- **Static production binary**: simple deploy, no Python environment drift, fewer runtime dependencies.
- **Type-checked protocols**: auth, budget, route, ledger, and proxy contracts are explicit at compile time.

The code intentionally avoids a plugin-heavy proxy architecture. The fastest profile keeps routing state in RAM, reloads control-plane state from Postgres, and writes usage asynchronously.

## Architecture

```text
client / SDK
    |
    |  OpenAI-compatible or Anthropic-compatible HTTP
    v
BrighTO-Router
    |
    |-- auth: validate client key from Authorization Bearer or x-api-key
    |-- request head scan: read only the fields needed for routing and policy
    |-- budget: RAM counters for team/key/model budgets, RPM, concurrency
    |-- route: choose backend from current config snapshot
    |-- proxy: forward request, preserve streaming, tap usage
    |-- ledger: enqueue usage event, write Postgres in background, fallback to JSONL file if DB is down
    |-- metrics/logs: Prometheus and structured JSON logs
    v
LLM backends
    |-- vLLM / llama-server / OpenAI-compatible servers
    |-- Anthropic Messages API
    |-- other compatible gateways
```

Production state is split deliberately:

- **Hot path**: RAM snapshots, RAM counters, reqwest connection pools, bounded streaming channels.
- **Control plane**: Postgres tables for backends, routes, teams, API keys, and usage ledger.
- **Background work**: config polling, health checks, ledger batching, metric gauge refresh.

Postgres is required for production configuration and ledger persistence. Redis is not part of the default fastest profile. Add Redis only if you need strict shared quota enforcement across multiple router instances and can prove the latency tradeoff is worth it for that deployment.

## Repository layout

```text
src/
  main.rs              # binary wiring, background tasks, graceful shutdown
  handlers.rs          # public HTTP routes, auth, request classification
  proxy/mod.rs         # backend forwarding, stream tap, usage parsing
  budget/mod.rs        # RAM budget/RPM/concurrency store
  route/mod.rs         # backend pool, circuit breaker, least-load selection
  ledger/mod.rs        # async Postgres ledger writer + fallback replay
  admin/mod.rs         # admin API and embedded portal
  config/mod.rs        # Postgres config loader and env/file backend key resolution
  bin/mock_upstream.rs # deterministic mock backend for tests and benchmarks

migrations/             # PostgreSQL schema
static/                 # embedded admin portal
scripts/                # production checks and canonical benchmark harness
benchmarks/             # benchmark spec, payload generator, threshold file
k8s/                    # minimal Kubernetes manifests
swarm/                  # build/audit history and agent-working artifacts
bench/                  # reviewed baseline plus generated benchmark results
```

## Install

The authoritative install path is the repository root. There is no separate setup folder and no `setup.sh` flow.

Prerequisites:

- Linux, ideally Ubuntu 24.04 for production parity.
- Docker with the Compose plugin.
- `curl` for health checks.
- Rust stable with rustc >= 1.94, `sqlx-cli`, `oha`, `psql`, and `jq` for development gates.

Fast start:

```bash
./start.sh start
```

`start.sh start` creates `.env` from `.env.example` if needed, starts PostgreSQL, runs migrations, pulls `thusinh1969/brighto_airouter:v1` if needed, and starts the router. The default admin key is `brighto-admin-dev` so a local Docker Compose stack starts immediately. Change `ADMIN_MASTER_KEY` before any shared or production deployment.

Direct Docker Compose commands are also supported:

```bash
cp .env.example .env
docker compose up -d postgres
./start.sh migrate
docker compose up -d router
```


## Docker image

The official runtime image is published on Docker Hub:

```text
thusinh1969/brighto_airouter:v1
```

Docker Compose pulls this image automatically by default. To build the official image tag locally before pushing:

```bash
make image
```

To test another image tag locally, set `BRIGHTO_ROUTER_IMAGE` before starting:

```bash
BRIGHTO_ROUTER_IMAGE=brighto-router:dev ./start.sh restart
```

## Simple operation with `start.sh`

The repository includes a small wrapper for common local/server operations. The default router image is `thusinh1969/brighto_airouter:v1`; override it with `BRIGHTO_ROUTER_IMAGE` if you build a local image.

```bash
./start.sh start      # start Postgres, run migrations, pull image if needed, then start router
./start.sh stop       # stop the stack
./start.sh restart    # rebuild/restart the stack
./start.sh status     # show container status and health endpoints
./start.sh logs       # follow router logs
./start.sh migrate    # run SQL migrations against DATABASE_URL
./start.sh smoke      # run a short non-release benchmark smoke
./start.sh gate       # run the release gate defined by Makefile
```

`start.sh start` creates `.env` automatically when it is missing. `status`, `stop`, and `logs` handle a missing `.env` cleanly. The default `.env.example` is intentionally runnable for local development; production operators must replace `ADMIN_MASTER_KEY` and backend key values.

## Configure a backend and model route

Run migrations first:

```bash
./start.sh migrate
```

Backends and model routes are currently configured in Postgres. Backend secrets are resolved from environment variables or files, so plaintext provider keys do not need to live in the database.

Example OpenAI-compatible backend:

```bash
export DATABASE_URL=postgres://brighto_router:brighto_router_dev@127.0.0.1:5432/brighto_router
export BACKEND_KEY_GX10=your-backend-key

psql "$DATABASE_URL" <<'SQL'
INSERT INTO backends (name, base_url, api_key_ref, weight, max_inflight, format, enabled)
VALUES ('gx10', 'http://127.0.0.1:8000', 'env:BACKEND_KEY_GX10', 1, 64, 'openai', true)
RETURNING id;
SQL
```

Then map a model alias to that backend id:

```bash
psql "$DATABASE_URL" <<'SQL'
INSERT INTO model_routes (model_name, backend_ids, fallback_backend_id, chars_per_token, first_byte_timeout)
VALUES ('qwen3.8-flash', '[1]', NULL, 4.0, 180)
ON CONFLICT (model_name) DO UPDATE SET
  backend_ids = EXCLUDED.backend_ids,
  fallback_backend_id = EXCLUDED.fallback_backend_id,
  chars_per_token = EXCLUDED.chars_per_token,
  first_byte_timeout = EXCLUDED.first_byte_timeout;
SQL
```

Example Anthropic backend:

```sql
INSERT INTO backends (name, base_url, api_key_ref, weight, max_inflight, format, enabled)
VALUES ('anthropic', 'https://api.anthropic.com', 'env:BACKEND_KEY_ANTHROPIC', 1, 0, 'anthropic', true);
```

Config reloads every `CONFIG_POLL_SECS` seconds. Admin mutations also trigger an immediate reload.

## Admin portal and API

Open the portal:

```text
http://127.0.0.1:8080/
http://127.0.0.1:8080/admin/
```

Admin requests require `x-admin-key: <ADMIN_MASTER_KEY>` and must come from an address allowed by `ADMIN_ALLOW_CIDR`.

Create a team:

```bash
curl -sS http://127.0.0.1:8080/admin/teams \
  -H "content-type: application/json" \
  -H "x-admin-key: $ADMIN_MASTER_KEY" \
  -d '{
    "name": "platform",
    "budget": {"period": "month", "max_tokens": 10000000, "per_model": {}},
    "enabled": true
  }'
```

Create an API key:

```bash
curl -sS http://127.0.0.1:8080/admin/keys \
  -H "content-type: application/json" \
  -H "x-admin-key: $ADMIN_MASTER_KEY" \
  -d '{
    "team_id": 1,
    "owner": "team@example.com",
    "allowed_models": ["qwen3.8-flash"],
    "rpm_limit": 600,
    "concurrency_limit": 32
  }'
```

The plaintext key is returned only once. Store it immediately.

Query usage:

```bash
curl -sS 'http://127.0.0.1:8080/admin/usage?team=1' \
  -H "x-admin-key: $ADMIN_MASTER_KEY" | jq
```

Disable a key:

```bash
curl -sS -X DELETE http://127.0.0.1:8080/admin/keys/1 \
  -H "x-admin-key: $ADMIN_MASTER_KEY" -i
```

## Client API

OpenAI-compatible request:

```bash
curl -sS http://127.0.0.1:8080/v1/chat/completions \
  -H "content-type: application/json" \
  -H "authorization: Bearer $BRIGHTO_API_KEY" \
  -d '{
    "model": "qwen3.8-flash",
    "messages": [{"role": "user", "content": "Say hello in one sentence."}],
    "stream": false
  }'
```

Streaming request:

```bash
curl -N http://127.0.0.1:8080/v1/chat/completions \
  -H "content-type: application/json" \
  -H "authorization: Bearer $BRIGHTO_API_KEY" \
  -d '{
    "model": "qwen3.8-flash",
    "messages": [{"role": "user", "content": "Stream a short answer."}],
    "stream": true
  }'
```

Anthropic-compatible request:

```bash
curl -sS http://127.0.0.1:8080/v1/messages \
  -H "content-type: application/json" \
  -H "x-api-key: $BRIGHTO_API_KEY" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-alias",
    "max_tokens": 128,
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

Model discovery:

```bash
curl -sS http://127.0.0.1:8080/v1/models \
  -H "authorization: Bearer $BRIGHTO_API_KEY" | jq
```

## Health, readiness, and metrics

```bash
curl -sS http://127.0.0.1:8080/healthz
curl -i  http://127.0.0.1:8080/readyz
curl -sS http://127.0.0.1:8080/metrics
```

`/healthz` reports whether the process is alive. `/readyz` reflects whether the control-plane config has loaded successfully and recently. The inference path uses the last good in-memory snapshot; a temporary Postgres outage should not make existing routed traffic wait on the database.

Prometheus metrics include request counts, token counts, TTFB, router overhead, backend inflight count, budget remaining, circuit state, and ledger drops.

## Benchmark truth

The project is benchmark-first. The full benchmark strategy lives in [benchmarks/STRATEGY.md](benchmarks/STRATEGY.md); the release contract lives in [benchmarks/BENCHMARK.md](benchmarks/BENCHMARK.md); thresholds live in [benchmarks/thresholds.toml](benchmarks/thresholds.toml). The canonical local performance harness is `scripts/bench_real.py` and the compatibility shell entrypoint is `benchmarks/gate.sh`.

The local benchmark uses a deterministic Rust mock upstream, `brighto-router-mock`, so router overhead is not hidden behind model latency. The benchmark generates 1k, 50k, and 200k-token-class payloads, measures direct-to-mock versus router-to-mock, and records raw `oha` JSON for audit.

Latest verified local artifact in this workspace:

```text
bench/results/20260917-000839
```

Run settings:

```text
DUR=60s
WARM=15s
RUNS=3
CONCS=1,50,200
BENCH_B6=1
```

Observed local mock gate numbers:

```text
1k   c=50 overhead p50 +0.288ms p99 +0.512ms
50k  c=50 overhead p50 +0.503ms p99 +0.734ms
200k c=50 overhead p50 +0.785ms p99 +0.813ms
B4 1k   TTFB delta +0.004ms
B4 50k  TTFB delta +0.041ms
B4 200k TTFB delta -0.017ms
B6 target throughput 8499.48 rps, non-200 0
```

The artifact passed the long local harness before the B10 ledger-lag, RSS memory, 500k/1M stress, and baseline-regression additions. The current harness now records B3 flatness, worst-run fields, router RSS memory samples, true B10 ledger lag with an isolated benchmark model, and baseline status. Rerun the full gate with `BASELINE_BOOTSTRAP=1` before treating the current tree as proof of every row in `BENCHMARK.md`.

Run a short smoke test:

```bash
./start.sh smoke
```

Run the release gate after a reviewed `bench/baseline.json` exists:

```bash
./start.sh gate
```

Create the first baseline only from a full reviewed green run:

```bash
BASELINE_BOOTSTRAP=1 ./start.sh gate
cp bench/results/<timestamp>/baseline_candidate.json bench/baseline.json
```

Run a large prompt stress proof for 500k and 1M token-class pass-through behavior:

```bash
BENCH_PAYLOADS=500k,1m \
BENCH_STREAM_PAYLOADS=500k-stream,1m-stream \
CONCS=1,50,200 \
RUNS=3 \
DUR=60s \
WARM=15s \
BENCH_B6=0 \
BENCH_B10=0 \
REQUIRE_PASS=0 \
python3 scripts/bench_real.py
```

This stress proof records router memory through RSS, which means resident set size reported by Linux. The stress proof is measurement-first until a reviewed baseline exists; it should not fail on made-up speed targets.

For public claims such as “fastest LLM router”, compare BrighTO-Router against named routers on the same hardware, same payloads, same backend, same TLS/proxy settings, and same offered-rate shape. The repository keeps raw artifacts so these claims can be audited rather than inferred.

## Testing

Current verified test inventory on this workspace:

```text
52 tests total
48 library tests
4 streaming/upload integration tests
```

The current integration tests cover:

```text
stream_request_taps_usage_and_forwards_sse
stream_without_usage_records_estimate_not_zero
large_nonstream_upload_preserves_exact_content_length
nonstream_response_starts_before_upstream_eof
```

The unit tests cover auth decisions, budget reservation/refund behavior, RPM buckets, concurrency guards, config reload behavior, ledger fallback/replay, metric-name guardrails, SSE usage parsing, route/fallback/circuit selection, and request-head parsing.

Run the standard checks:

```bash
python3 scripts/hotpath_guard.py
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
DATABASE_URL=postgres://brighto_router:brighto_router_dev@127.0.0.1:5432/brighto_router \
  cargo test --locked --all-targets
```

`make test` uses `scripts/test_postgres.sh`; it expects a reachable PostgreSQL database through `DATABASE_URL`, or the default local Compose database after `./start.sh start`.

## How we built it

BrighTO-Router was built from the benchmark contract backward:

1. Define the release gates in `benchmarks/BENCHMARK.md` before optimizing.
2. Build a deterministic Rust mock upstream so router overhead can be measured without model noise.
3. Add raw benchmark artifacts for every direct and routed run.
4. Audit the hot path for full-body buffering, DB awaits, lock/await hazards, header leaks, retry-after-first-byte hazards, and accidental compression/decompression.
5. Fix measured root causes with the smallest code changes that preserve streaming semantics.
6. Reject larger rewrites when they improve one run but fail the full gate or add unnecessary architecture.
7. Keep production dependencies minimal: Rust binary + Postgres for control/ledger persistence.

The most important performance fixes and review notes are documented in `swarm/audits/`. The current fast path is based on exact-length upload passthrough, no backend gzip decode, small-response fast path, streamed large responses, 4 Tokio worker threads for the measured machine, in-memory config snapshots, in-memory counters, and async ledger writes.

## SME/team use cases

BrighTO-Router is useful when a small or mid-sized engineering team needs:

- One internal LLM endpoint for many apps.
- Provider key isolation: client keys never become backend keys.
- Team budgets and per-key limits without changing application code.
- Model aliases such as `fast`, `long-context`, `vision`, or `backup` mapped to real backends.
- Controlled fallback between local GPUs and cloud providers.
- Usage visibility by team, key, model, status, and latency.
- A simple admin portal for day-to-day key and budget operations.
- Prometheus metrics for existing observability stacks.
- One-command local/server operation through `./start.sh start|stop|status|logs|migrate|smoke|gate`.

## Enterprise roadmap

The open-source edition focuses on the fastest single-router or small-cluster profile. Enterprise features should be added only when they preserve the hot-path design and come with benchmark evidence.

Planned enterprise capabilities:

- SSO/SAML/OIDC for the admin portal.
- RBAC for org, workspace, team, and environment scopes.
- Audit log export and immutable admin-event history.
- Multi-instance global quotas with Redis/Valkey or another low-latency shared counter store, behind an explicit feature flag and benchmark gate.
- Policy-as-code for model allowlists, data zones, and provider routing rules.
- Multi-tenant portal with billing views and cost allocation.
- HA deployment templates for Kubernetes and systemd.
- Secret manager integrations for backend keys.
- mTLS/private networking examples for enterprise backends.
- Provider-specific conformance suites for cloud LLM APIs.

## Kubernetes

A minimal Kubernetes deployment is provided in `k8s/`. It is intentionally small: a Deployment, Service, ConfigMap example, and Secret example. Use it when you already run PostgreSQL outside the pod through a managed database or a dedicated PostgreSQL StatefulSet. The router image defaults to `thusinh1969/brighto_airouter:v1` and expects the same environment variables as Docker Compose.

```bash
kubectl create namespace brighto-router
kubectl -n brighto-router apply -f k8s/postgres.dev.yaml    # local/dev only
kubectl -n brighto-router apply -f k8s/secret.example.yaml
kubectl -n brighto-router apply -f k8s/configmap.example.yaml
kubectl -n brighto-router apply -f k8s/deployment.yaml
kubectl -n brighto-router apply -f k8s/service.yaml
```

Before production, replace the example secret, point `DATABASE_URL` at a production PostgreSQL endpoint, and run migrations during your release process. Do not add Redis to the pod path unless strict global quotas across multiple router replicas are required and benchmarked.

## Production notes

- Put TLS termination at a battle-tested edge such as nginx, Envoy, Caddy, or a cloud load balancer unless you have a reason to terminate TLS in the binary.
- Keep Docker Compose `network_mode: host` for lowest local overhead where appropriate; measure before changing networking mode.
- Set `ADMIN_MASTER_KEY` to a strong secret and restrict `ADMIN_ALLOW_CIDR`.
- Store backend provider keys in environment variables or files referenced by `api_key_ref`.
- Monitor `router_ledger_dropped_total`; it must stay at zero.
- Keep `LEDGER_FALLBACK_FILE` on persistent storage so usage can be replayed after a database outage.
- Do not add Redis, queues, caches, or policy engines to the inference path unless a production requirement and benchmark evidence justify them.

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) for development and benchmark expectations. See [SECURITY.md](SECURITY.md) for vulnerability reporting and deployment security notes.

## License

Apache-2.0. See [LICENSE](LICENSE).
