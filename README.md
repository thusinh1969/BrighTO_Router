# BrighTO-Router

[![CI](https://github.com/thusinh1969/Brighto_AIRouter/actions/workflows/ci.yml/badge.svg)](https://github.com/thusinh1969/Brighto_AIRouter/actions/workflows/ci.yml)

BrighTO-Router is a Rust LLM router for teams that want one fast, controlled internal endpoint for many model providers.

An **LLM** is a Large Language Model. A **router** is the service between your application and model providers. BrighTO-Router receives OpenAI-style or Anthropic-style API requests, chooses the configured backend, forwards the request, streams the response, and records usage.

The design goal is simple: keep the request path short enough that the router is not the bottleneck. PostgreSQL stores configuration and usage. The production runtime is the Rust router plus PostgreSQL.

Official repository: `https://github.com/thusinh1969/Brighto_AIRouter`

Official Docker image: `thusinh1969/brighto_airouter:v1`

Release version: `1.0.0`

## Quick start

Prerequisites: Linux, Docker, Docker Compose plugin, and `curl`.

```bash
git clone https://github.com/thusinh1969/Brighto_AIRouter.git
cd Brighto_AIRouter
./start.sh install
./start.sh status
```

Open the portal on the same server:

```text
http://127.0.0.1:18080/
```

From another machine, use the server IP:

```text
http://<SERVER_IP>:18080/
```

Default local admin key:

```text
brightoIsGreat@2026
```

Fresh install allows Admin Portal access from any IP so first-time remote testing works immediately. Before shared or production use, replace `ADMIN_MASTER_KEY` and narrow `ADMIN_ALLOW_CIDR` in `.env` to your VPN, office subnet, or reverse proxy, then run `./start.sh restart`.

## What install creates by default

`./start.sh install` creates `.env` from `.env.example`, starts PostgreSQL in Docker, runs migrations, seeds basic records, pulls `thusinh1969/brighto_airouter:v1` when needed, and starts the router.

Default database records:

| Record | Created value | Purpose |
|---|---|---|
| Team | `Default Team` | Lets a first-time admin create client API keys immediately. |
| Demo client key | `lc-0123456789abcdef0123456789abcdef` | Local smoke testing only. Replace or disable it before shared use. |
| Model routes | None | You decide which provider model is exposed to clients. |
| Provider templates | Disabled presets for OpenAI, Anthropic, Gemini, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Meta Muse, and Custom LLM | Friendly defaults for the Portal dropdown. They are not active routes until you save a tested model route. |

The provider catalog is read from `.env` through `PROVIDER_CATALOG`. It gives the Portal a friendly dropdown for OpenAI, Anthropic, Gemini, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Meta Muse, and Custom LLM. The catalog is a preset list, not a route by itself.

`./start.sh start`, `./start.sh restart`, and Docker image rebuilds do not wipe PostgreSQL. Local data is kept in the Docker named volume `brighto-airouter_pg-data`. Data is removed only if you explicitly delete the volume, run `docker compose down -v`, or execute a manual reset/truncate SQL. The default seed is safe to rerun: it inserts missing provider templates only and does not overwrite user-edited providers or model routes.

To add a model, open **Models & Routes → Add model**:

1. Pick a provider preset or **Custom LLM**.
2. Enter or accept the Base URL.
3. Paste the provider API key, or leave it blank to use the matching `.env` key if one is configured.
4. Click **Load models**, choose one provider model, then click **Test connection**.
5. Save enabled only after the test passes.

The provider API key belongs to the model route. Admin can paste it in the wizard; cloud defaults may also come from `.env` variables such as `OPENAI_API_KEY`.

## Install options

Local Docker Postgres plus router:

```bash
./start.sh install
```

Existing PostgreSQL database:

```bash
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router'
```

Kubernetes starter deployment:

```bash
./start.sh install --k8s --replicas 2
```

Kubernetes with an existing PostgreSQL database:

```bash
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router' --k8s --replicas 2
```

Daily commands:

```bash
./start.sh start
./start.sh stop
./start.sh restart
./start.sh status
./start.sh logs
./start.sh seed
./start.sh smoke
```

More detail: [INSTALL.md](INSTALL.md), [HTTPS.md](HTTPS.md), [PROVIDERS.md](PROVIDERS.md), [k8s/README.md](k8s/README.md).

## Portal front-end development

The Portal is intentionally simple: one file contains the HTML, CSS, and JavaScript:

```text
static/index.html
```

In production, Rust embeds this file into the `brighto-router` binary. That keeps deployment to one Docker image and avoids a separate Node/React build pipeline.

For live UI design work, enable disk-backed Portal mode in `.env`:

```bash
PORTAL_STATIC_FILE=/app/static/index.html
docker compose up -d --force-recreate router
```

`docker-compose.yml` mounts `./static` into the container at `/app/static`. After this one restart, edit `static/index.html` and press F5 in the browser. You only need to rebuild Docker again when Rust code changes or when you want to bake the final HTML into the production image.

## Optional HTTPS with custom PEM files

For a private Ubuntu server, you can place your certificate and key under `ssl/`, mount that directory into Docker, and set `TLS_CERT_PATH` plus `TLS_KEY_PATH` in `.env`.

If you do not have a real certificate yet, create a local self-signed certificate:

```bash
SERVER_IP=$(hostname -I | awk '{print $1}')
mkdir -p ssl
openssl req -x509 -newkey rsa:4096 -sha256 -days 3650 -nodes \
  -keyout ssl/privkey.pem \
  -out ssl/fullchain.pem \
  -subj "/CN=${SERVER_IP}" \
  -addext "subjectAltName=IP:${SERVER_IP},IP:127.0.0.1,DNS:localhost,DNS:brighto-router"
chmod 600 ssl/privkey.pem
chmod 644 ssl/fullchain.pem
```

Then follow [HTTPS.md](HTTPS.md) for the full setup.

## What it offers a team

BrighTO-Router gives small and medium-sized teams one controlled model gateway:

- One internal API endpoint for many providers.
- OpenAI-style routes: `/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `/v1/models`.
- Anthropic Messages route: `/v1/messages`.
- Model aliases, weighted backend routing, fallback backend support, and circuit breaking.
- Team budgets, API-key budgets, per-model budget support, request-per-minute limits, and concurrency limits.
- Usage ledger in PostgreSQL with file fallback if Postgres is temporarily unavailable.
- Admin portal for provider setup, model route setup, teams, client API keys, budgets, and usage lookup.
- Health endpoints: `/healthz`, `/readyz`.
- Metrics endpoint: `/metrics`.

Current media support:

| Input type | Status |
|---|---|
| Text chat/completions | Supported. |
| Embeddings | Supported. |
| Anthropic messages | Supported. |
| Image data inside chat JSON | Passes through when the backend accepts that JSON and the body stays under `MAX_BODY_BYTES`. |
| Dedicated image/audio/video APIs | Future work. |

## Why Rust instead of Python

A router should forward bytes, preserve streaming, make fast routing decisions, and update counters without unpredictable per-request overhead. Rust is a better fit for this job than Python because it gives:

- a static production binary;
- predictable memory use for large prompts;
- safe high-concurrency networking with Tokio;
- no Python package/runtime drift in deployment;
- compile-time checks around routing, budget, ledger, and proxy contracts.

Python remains useful for benchmark scripts and operational tooling. The request path is Rust.

## How it is built

```text
Client application
  -> BrighTO-Router
     -> authenticate client key
     -> read only the request fields needed for routing and policy
     -> check team/key budget and concurrency limits in memory
     -> choose a healthy backend from the current config snapshot
     -> forward the request and response stream
     -> write usage asynchronously to PostgreSQL
  -> LLM provider or local model server
```

Runtime state is split deliberately:

| Part | Where it lives | Why |
|---|---|---|
| Routing snapshot | Memory | The hot request path should not wait on the database. |
| Budgets and live counters | Memory | Fast admission checks. |
| Backends, routes, teams, keys | PostgreSQL | Durable production control plane. |
| Usage ledger | PostgreSQL | Durable cost/accounting record. |
| Ledger fallback | Local JSONL file | Keeps serving when Postgres has a temporary outage. |

**JSONL** means one JSON record per line.

## Benchmark strategy

The benchmark is designed to answer a plain question: how much delay does the router add compared with calling the backend directly?

The benchmark always compares two paths on the same machine:

```text
client -> mock backend
client -> BrighTO-Router -> same mock backend
```

The mock backend is a deterministic Rust server. It returns immediately, so model inference time does not hide router overhead.

Term legend:

| Term | Meaning |
|---|---|
| Payload | Request body size. `1k` means about 1,000 input tokens; `1m` means about 1,000,000 input tokens. |
| Concurrency | Maximum number of active requests at the same time. |
| Offered rate | The request rate the load generator tries to start. Example: `4000 RPS` means it tries to start 4,000 requests per second. |
| RPS | Requests per second. |
| p50 | Median result: half the requests are faster, half are slower. |
| p99 | 99th percentile result: 99% of requests are faster, 1% are slower. |
| TTFB | Time to first byte: how long the client waits for the first response byte. |
| RSS | Resident set size: physical memory used by the router process on Linux. |

`1k c=50 offered rate 4000 RPS` means: use the 1k-token payload, allow at most 50 active requests, and ask the load generator to start up to 4,000 requests per second. This is not a global standard number. It is a local calibrated load shape. The standard part is the method: compare direct backend versus router on the same hardware, same payload, same concurrency, same run duration, same logs, and same network path.

Benchmark matrix:

| Payload | Concurrency levels | Main measurement | Why it matters |
|---|---:|---|---|
| `1k` | 1, 50, 200 | latency overhead, throughput, ledger lag | Normal team traffic must stay fast. |
| `50k` | 1, 50, 200 | latency overhead, streaming first byte, memory | Retrieval and agent prompts must stay pass-through. |
| `200k` | 1, 50, 200 | latency overhead, streaming first byte, memory flatness | Large context must not create proportional router delay. |
| `500k` | 1, 50, 200 after review | correctness, memory, overhead | Extreme prompts should not corrupt data or grow memory unexpectedly. |
| `1m` | 1, 50, 200 after review | correctness, memory, overhead | Capacity planning for very large prompts on high-memory servers. |

Current verified public-facing status:

- 1k, 50k, and 200k release-gate payloads have measured sub-millisecond local mock overhead in the existing benchmark artifacts.
- 500k and 1M payload generation and mock pass-through have been verified in smoke mode.
- The full 500k and 1M production proof must be run on the target dual-Xeon server before publishing claims for those sizes.
- “Fastest in the world” should only be claimed after public same-machine comparisons against named routers.

Benchmark docs:

- Simple benchmark guide: [benchmarks/README.md](benchmarks/README.md)
- Release benchmark contract: [benchmarks/BENCHMARK.md](benchmarks/BENCHMARK.md)
- Full strategy: [benchmarks/STRATEGY.md](benchmarks/STRATEGY.md)

## Enterprise direction

The open-source edition focuses on the fast router, PostgreSQL-backed control plane, local/admin setup, provider templates, and transparent benchmark artifacts.

The enterprise edition is the right place for features that teams usually need after adoption:

- SSO: Single Sign-On through OIDC or SAML.
- RBAC: role-based admin permissions.
- Organization and project hierarchy.
- Approval workflow for provider/model changes.
- Central audit log export.
- Secrets manager integration.
- Multi-region deployment guidance.
- Provider cost dashboards and chargeback reports.
- Support packages and performance certification on customer hardware.

## Development

```bash
make test
make check
./start.sh smoke
```

`make test` starts a temporary PostgreSQL container automatically when the default local database is not already reachable. The release gate is stricter:

```bash
BASELINE_BOOTSTRAP=1 ./start.sh gate
```

Review the generated files under `bench/results/<timestamp>/` before turning a candidate into a committed baseline.

## License

See [LICENSE](LICENSE).
