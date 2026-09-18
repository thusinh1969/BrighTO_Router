# BrighTO-Router

**The ultra-fast, self-hosted LLM router for teams that want speed, control, and simple operations.**

BrighTO-Router gives your team one clean endpoint for OpenAI-compatible, Anthropic-compatible, cloud, and local models. It is built in Rust for low-overhead pass-through, uses PostgreSQL as the single durable store, and scales by running stateless router replicas behind a load balancer.

| V1.0 proof point | Result |
|---|---:|
| 200k-token mock pass-through, 50 concurrent requests | `0.958 ms p99 router overhead` |
| 50k-token mock pass-through, 50 concurrent requests | `0.707 ms p99 router overhead` |
| 1k-token mock pass-through, 50 concurrent requests | `0.418 ms p99 router overhead` |
| Router memory during the release artifact | `26.12 MB max RSS` |

Measured on Intel Xeon Gold 6148 using the same-machine benchmark artifact at `bench/results/20260917-025033/summary.json`.

Official repository: `https://github.com/thusinh1969/Brighto_AIRouter`

Official Docker image: `thusinh1969/brighto_airouter:v1`

Release version: `1.0.0`

## Why BrighTO-Router

Broad AI gateways are useful when you need a huge provider catalog, hosted accounts, prompt tooling, agent tooling, and enterprise workflow in one platform. BrighTO-Router is for teams with a sharper requirement: run a very fast gateway they control, with a clean Portal, transparent usage, and a production stack small enough to understand.

For V1.0, the product promise is deliberately narrow and strong: Rust on the request path, PostgreSQL for durable state, stateless router replicas for horizontal scale, one Docker image, one install script, and benchmark artifacts that compare router latency against a direct backend on the same machine.

| If you need... | BrighTO-Router gives you... |
|---|---|
| One endpoint for team apps | OpenAI-compatible and Anthropic-compatible routes through one router. |
| A simple self-hosted install | `./start.sh install` starts PostgreSQL, runs migrations, seeds provider templates, and starts the router. |
| Fast pass-through behavior | Rust hot path, streaming proxy, in-memory routing snapshot, async PostgreSQL ledger writes. |
| Cost and usage control | Teams, visible client API keys, budgets, expiry, request-per-minute limits, and concurrency limits. |
| Provider setup without YAML pain | Portal flow: choose provider, paste API key, load models, test connection, save one model route. |
| Honest benchmarking | Same-machine direct-backend versus router-backend tests, from small prompts to very large payloads. |
| A clean production dependency model | Rust router + PostgreSQL as the required datastore. |
| Scale beyond one box | Stateless router instances can run as multiple Docker/Kubernetes replicas behind a load balancer. |

BrighTO-Router is not trying to win by listing hundreds of integrations. It is trying to be the router a serious team can understand, run, audit, and tune.

## Benchmark proof

Method: same client, same machine, same mock backend, direct call versus router call. Release artifact: `bench/results/20260917-025033/summary.json` on Intel Xeon Gold 6148.

| Payload | Concurrency | p50 router overhead | p99 router overhead | Streaming first-byte delta | Router memory max |
|---|---:|---:|---:|---:|---:|
| `1k` | 50 | `0.287 ms` | `0.418 ms` | `-0.017 ms` | `26.12 MB` |
| `50k` | 50 | `0.544 ms` | `0.707 ms` | `-0.015 ms` | `26.12 MB` |
| `200k` | 50 | `0.901 ms` | `0.958 ms` | `-0.010 ms` | `26.12 MB` |

The benchmark also verified PostgreSQL ledger writing at 200 requests per second with 200 expected rows and 200 observed rows in that release artifact. Larger 500k and 1M token-class payloads are supported by the benchmark harness and smoke-tested, but they are not promoted to public speed claims until full dual-Xeon production proof is reviewed.

## Quick start

Prerequisites: Linux, Docker, Docker Compose plugin, and `curl`.

```bash
git clone https://github.com/thusinh1969/Brighto_AIRouter.git
cd Brighto_AIRouter
./start.sh install
```

Open the Portal on the server:

```text
http://127.0.0.1:18080/
```

Open it from another machine:

```text
http://<SERVER_IP>:18080/
```

Default local admin key:

```text
brightoIsGreat@2026
```

Fresh install allows Admin Portal access from any IP so first-time remote testing works immediately. Before shared or production use, replace `ADMIN_MASTER_KEY` and narrow `ADMIN_ALLOW_CIDR` in `.env` to your VPN, office subnet, or reverse proxy, then run:

```bash
./start.sh restart
```

Check the stack at any time:

```bash
./start.sh status
```

`status` prints the active Portal URL, local health checks, and the command to test from another machine.

## First model route

Open **Models & Routes → Add model** in the Portal.

1. Choose a provider preset such as OpenAI, Anthropic, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, or **Custom LLM**.
2. Accept the default Base URL or enter your own.
3. Paste the provider API key, or leave it blank when the matching `.env` key is already set.
4. Click **Load models**.
5. Select one model.
6. Click **Test connection**.
7. Save the route.

The provider API key belongs to the model route. Client applications do not receive provider keys. They call BrighTO-Router with a client API key issued from the **API Keys** screen.

## What install creates

`./start.sh install` creates `.env` from `.env.example`, starts PostgreSQL in Docker, runs migrations, seeds default records, pulls `thusinh1969/brighto_airouter:v1`, and starts the router.

Default records:

| Record | Created value | Purpose |
|---|---|---|
| Team | `Default Team` | Lets an admin create client API keys immediately. |
| Demo client key | `lc-0123456789abcdef0123456789abcdef` | Local smoke testing only. Replace or disable it before shared use. |
| Model routes | None | You choose which provider models clients can call. |
| Provider endpoints | OpenAI, Anthropic, Gemini, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Meta Muse, Custom LLM | Friendly defaults for the Portal. They are not usable routes until a tested model route is saved. |
| Provider catalog | `PROVIDER_CATALOG` in `.env` | Controls the Add model provider dropdown. |

`./start.sh start`, `./start.sh restart`, Docker image pulls, and Docker image rebuilds do **not** wipe PostgreSQL. Local data is stored in the Docker named volume `brighto-airouter_pg-data`. Data is removed only when you explicitly delete the volume, run `docker compose down -v`, or manually reset the database.

## HTTP first, HTTPS when ready

Most users should start with HTTP, confirm the Portal works, then enable HTTPS.

HTTP is the default:

```text
http://<SERVER_IP>:18080/
```

To generate a local self-signed certificate and switch to HTTPS:

```bash
./start.sh make-self-signed-cert <SERVER_IP_OR_HOSTNAME>
./start.sh tls --cert ssl/fullchain.pem --key ssl/privkey.pem --host <SERVER_IP_OR_HOSTNAME> --port 18443
```

Then open:

```text
https://<SERVER_IP_OR_HOSTNAME>:18443/
```

Browsers will warn on a self-signed certificate. Use a real certificate for shared or production use. Full guide: [HTTPS.md](HTTPS.md).

## Daily operation

```bash
./start.sh start       # start PostgreSQL when local, migrate, seed, start router
./start.sh stop        # stop the Docker Compose stack
./start.sh restart     # migrate, seed, recreate router
./start.sh status      # show containers plus health and ready checks
./start.sh logs        # follow router logs
./start.sh seed        # seed missing defaults without overwriting your routes
./start.sh smoke       # run a short benchmark smoke test
```

Install variants:

```bash
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router'
./start.sh install --k8s --replicas 2
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router' --k8s --replicas 2
```

More detail: [INSTALL.md](INSTALL.md), [HTTPS.md](HTTPS.md), [PROVIDERS.md](PROVIDERS.md), [k8s/README.md](k8s/README.md).

## What teams get in V1.0

- One internal endpoint for multiple model providers.
- OpenAI-style routes: `/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `/v1/models`.
- Anthropic Messages route: `/v1/messages`.
- Model aliases and provider-backed model routes.
- Weighted backend routing, fallback backend support, and circuit breaking.
- Team budgets and API-key budgets.
- API-key expiry, request-per-minute limits, and concurrency limits.
- PostgreSQL usage ledger with file fallback if PostgreSQL is temporarily unavailable.
- Admin Portal for providers, model routes, teams, API keys, usage, and budgets.
- User Portal for issued client keys.
- Health endpoints: `/healthz`, `/readyz`.
- Prometheus metrics endpoint: `/metrics`.

Current media support:

| Input type | Status |
|---|---|
| Text chat/completions | Supported. |
| Embeddings | Supported. |
| Anthropic messages | Supported. |
| Image data inside chat JSON | Passed through when the backend accepts that JSON and the body stays under `MAX_BODY_BYTES`. |
| Dedicated image/audio/video APIs | Future work. |

## Why Rust instead of Python

A router spends most of its life moving bytes, preserving streams, applying small policy decisions, and avoiding avoidable per-request overhead. Rust is a strong fit for that job.

BrighTO-Router uses Rust for the request path because it gives:

- predictable memory behavior under large payloads;
- safe high-concurrency networking with Tokio;
- a single production binary inside one Docker image;
- no Python package/runtime drift in production;
- compile-time checks around routing, budget, ledger, and proxy contracts.

Python is still useful for benchmark scripts and operational tooling. It is not in the production request path.

## How it works

```text
Client application
  -> BrighTO-Router
     -> authenticate client API key
     -> read only the fields needed for routing and policy
     -> check team/key budget and concurrency limits
     -> choose a healthy provider endpoint from the current config snapshot
     -> forward the request and stream the response
     -> write usage asynchronously to PostgreSQL
  -> model provider or local model server
```

Runtime state is split deliberately:

| Part | Where it lives | Why |
|---|---|---|
| Routing snapshot | Memory | The hot request path should not wait on the database. |
| Budgets and live counters | Memory | Fast admission checks. |
| Provider endpoints, routes, teams, keys | PostgreSQL | Durable control plane. |
| Usage ledger | PostgreSQL | Durable cost and usage record. |
| Ledger fallback | Local JSONL file | Keeps serving during a temporary PostgreSQL outage. |

**JSONL** means one JSON record per line.

## Benchmark strategy

Do not trust vague gateway speed claims. Measure the router against a direct backend on the same machine.

BrighTO-Router’s benchmark compares two paths:

```text
client -> mock backend
client -> BrighTO-Router -> same mock backend
```

The mock backend is a deterministic Rust server. It returns quickly, so model inference time does not hide router overhead.

Term legend:

| Term | Meaning |
|---|---|
| Payload | Request body size. `1k` means about 1,000 input tokens; `1m` means about 1,000,000 input tokens. |
| Concurrency | Maximum number of active requests at the same time. |
| Offered rate | The request rate the load generator tries to start. Example: `4000 RPS` means it tries to start 4,000 requests per second. |
| RPS | Requests per second. |
| p50 | Median result: half the requests are faster, half are slower. |
| p95 | 95th percentile result: 95% of requests are faster, 5% are slower. |
| p99 | 99th percentile result: 99% of requests are faster, 1% are slower. |
| TTFB | Time to first byte: how long the client waits for the first response byte. |
| RSS | Resident set size: physical memory used by the router process on Linux. |

`1k c=50 offered rate 4000 RPS` means: use the 1k-token payload, allow at most 50 active requests, and ask the load generator to start up to 4,000 requests per second. It is a local calibrated load shape, not a universal standard. The standard part is the method: same hardware, same payload, same concurrency, same run duration, same backend, same network path.

Benchmark matrix:

| Payload | Concurrency levels | Main measurement | Why it matters |
|---|---:|---|---|
| `1k` | 1, 50, 200 | Latency overhead, throughput, ledger lag | Normal team traffic must stay fast. |
| `50k` | 1, 50, 200 | Latency overhead, first byte, memory | Retrieval and agent prompts must stay pass-through. |
| `200k` | 1, 50, 200 | Latency overhead, first byte, memory flatness | Large-context calls must not create proportional router delay. |
| `500k` | 1, 50, 200 after review | Correctness, memory, overhead | Extreme prompts should not corrupt data or grow memory unexpectedly. |
| `1m` | 1, 50, 200 after review | Correctness, memory, overhead | Capacity planning for very large prompts on high-memory servers. |

Current verified public-facing status:

- 1k, 50k, and 200k release-gate payloads measured sub-millisecond p99 local mock overhead in `bench/results/20260917-025033/summary.json`.
- 500k and 1M payload generation and mock pass-through have been verified in smoke mode.
- Full 500k and 1M production proof should be run on the target dual-Xeon server before publishing claims for those sizes.
- “Fastest in the world” should be claimed only after public same-machine comparisons against named routers.

Benchmark docs:

- Simple benchmark guide: [benchmarks/README.md](benchmarks/README.md)
- Release benchmark contract: [benchmarks/BENCHMARK.md](benchmarks/BENCHMARK.md)
- Full strategy: [benchmarks/STRATEGY.md](benchmarks/STRATEGY.md)

## Portal front-end development

The Portal is one file:

```text
static/index.html
```

It contains HTML, CSS, and JavaScript. Rust embeds this file into the production binary so the final release is still one Docker image.

For live UI design work, enable disk-backed Portal mode in `.env`:

```bash
PORTAL_STATIC_FILE=/app/static/index.html
./start.sh restart
```

`docker-compose.yml` mounts `./static` into the container at `/app/static`. After the restart, edit `static/index.html` and press F5 in the browser. Rebuild Docker only when Rust code changes or when you want the final Portal baked into the production image.

## Enterprise direction

The open-source edition focuses on the fast router, PostgreSQL-backed control plane, local/team setup, provider templates, the Portal, and transparent benchmark artifacts.

Enterprise features belong in a separate edition or service package:

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

The release gate is stricter:

```bash
BASELINE_BOOTSTRAP=1 ./start.sh gate
```

Review generated files under `bench/results/<timestamp>/` before turning a candidate into a committed baseline.

## License

See [LICENSE](LICENSE).
