# BrighTO-Router

**Million-token AI traffic, simple Rust fast path, one Docker install.**

Think NGINX-style reverse proxy for AI traffic, with model routing, team budgets, usage analytics, provider-key isolation, and a clean admin Portal built in. **BrighTO-Router preview-3** gives your team one endpoint for OpenAI-compatible chat/completions/embeddings, Anthropic Messages, provider-specific rerank adapters for search ranking, OpenAI-compatible ASR/transcription, cloud models, local models, and preview-3 Model Groups for simple chat endpoint load balancing. It is built in Rust for low-overhead pass-through, uses PostgreSQL as the single durable store, and scales by running stateless router replicas behind a load balancer.

| Benchmark proof point | Result |
|---|---:|
| 1M-token coding-context HTTP pass-through, 200 concurrent requests | `+2.167 ms p50`, `+1.664 ms p99` router overhead |
| 1M-token coding-context HTTPS pass-through, 200 concurrent requests | `+6.539 ms p50`, `+35.845 ms p99` router overhead |
| Streaming first-byte delta at 1M over HTTP | `-0.038 ms` |
| High-rate small-prompt saturation | `8,496 RPS`, `0` non-200 responses |
| PostgreSQL ledger at 2,000 RPS | `15,964 / 15,999` rows observed, `0` drops |
| Router memory during full 1M matrix | `60.04 MB` HTTP, `83.24 MB` HTTPS max RSS |

Measured on Intel Xeon Gold 6148 using same-machine mock-backend artifacts:

- HTTP: `benchmarks/artifacts/preview-3-http-1m-coding-context-summary.json`
- HTTPS: `benchmarks/artifacts/preview-3-https-1m-coding-context-summary.json`

These are pass-through benchmarks against a deterministic local Rust mock backend. They measure router overhead, not model inference speed, and they do not call paid cloud providers.

Official repository: `https://github.com/thusinh1969/BrighTO_Router`

Official Docker image for preview-3: `thusinh1969/brighto_airouter:preview-3`

Release version: `1.0.0-preview.3` (latest preview; intended to become `main` after final field feedback)

Preview-3 is the current public preview line. It keeps the million-token Rust fast path, adds first-class Model Groups for OpenAI-compatible chat load balancing, and includes task-aware Portal/API flows for embeddings, rerank, and ASR/transcription. See [ADAPTERS.md](ADAPTERS.md).

## What is new in preview-3

Preview-3 focuses on practical production routing without making the router harder to operate.

| New capability | What it does | Why it matters |
|---|---|---|
| Model Groups | One public OpenAI-compatible chat model can balance across two or more tested backend endpoints. | Apps keep using the same model name while the router spreads load and fails over when an endpoint is unhealthy. |
| Round-robin and weighted routing | Choose simple rotation or assign more traffic to stronger endpoints. | A local GPU endpoint, DeepSeek, OpenAI-compatible server, or other compatible backend can share traffic predictably. |
| Task-aware adapters | Embeddings, rerank, and ASR/transcription have explicit task types and Test Connection probes. | Admins stop guessing whether a model should be tested through chat, embeddings, rerank, or multipart ASR. |
| Cleaner Portal setup | Add model route and Create Model Group are primary actions on the Models page. | A new admin can create one working route or one load-balanced group without touching SQL or YAML. |
| One-line install defaults | The installer pulls the preview-3 Docker image, starts PostgreSQL, runs migrations, seeds provider templates, and opens the Portal. | A team can test locally first, then add HTTPS or Kubernetes only when needed. |

## Why BrighTO-Router

Broad AI gateways are useful when you need a huge provider catalog, hosted accounts, prompt tooling, agent tooling, and enterprise workflow in one platform. BrighTO-Router is for teams with a sharper requirement: run a very fast gateway they control, with a clean Portal, transparent usage, and a production stack small enough to understand.

For the preview line, the product promise is deliberately narrow and strong: Rust on the request path, PostgreSQL for durable state, stateless router replicas for horizontal scale, one Docker image, one install script, and benchmark artifacts that compare router latency against a direct backend on the same machine.

Pass-through means the router forwards supported request JSON and provider responses as-is, including streaming responses. It only reads the small fields needed for authentication, route selection, policy, budgets, usage metadata, and adapter-specific protocol mapping. For OpenAI-style streaming chat, BrighTO-Router forwards provider chunks back to the client as they arrive instead of waiting for the full response.

| If you need... | BrighTO-Router gives you... |
|---|---|
| One endpoint for team apps | Chat, completions, embeddings, rerank, ASR, and Anthropic Messages through one router. |
| A simple self-hosted install | `./start.sh install` starts PostgreSQL, runs migrations, seeds provider templates, and starts the router. |
| Fast pass-through behavior | Rust hot path, streaming proxy, in-memory routing snapshot, async PostgreSQL ledger writes. |
| Cost and usage control | Teams, visible client API keys, budgets, expiry, request-per-minute limits, and concurrency limits. |
| Provider setup without YAML pain | Portal flow: choose task type, choose provider, paste API key, load or type one model, test connection, save one route. |
| Honest benchmarking | Same-machine direct-backend versus router-backend tests, from small prompts to very large payloads. |
| A clean production dependency model | Rust router + PostgreSQL as the required datastore. |
| Scale beyond one box | Stateless router instances can run as multiple Docker/Kubernetes replicas behind a load balancer. Model Groups can also balance one public OpenAI-compatible chat model across multiple compatible backend endpoints. |

BrighTO-Router is not trying to win by listing hundreds of integrations. It is trying to be the router a serious team can understand, run, audit, and tune.

## Built for two real workloads

BrighTO-Router is designed to stay fast in both common team traffic and heavy coding-agent traffic. These workloads stress a router in different ways, so the benchmark suite measures both small and large payload behavior.

| Workload | Example | Why BrighTO-Router fits |
|---|---|---|
| Many concurrent users with small or average conversations | A 100-person team using chat, short multi-turn prompts, OpenAI-compatible embedding calls, and normal app traffic throughout the day. | The router keeps the hot path small: authenticate, check policy, choose a route, stream the response, and write usage asynchronously. |
| Many developers or coding agents with large contexts | Vibe-coding sessions, repository analysis, long prompts, retrieval-heavy requests, and multiple developers using large-context models at once. | Large JSON bodies are passed through without transforming media or rewriting prompt content, so router overhead stays low even when the backend receives much larger context. |

The verified benchmark artifacts cover `1k`, `50k`, `200k`, `500k`, and `1m` token-class coding-context payloads at concurrency 1, 50, and 200, over both HTTP and HTTPS. That range covers normal chat traffic, retrieval-heavy prompts, and large vibe-coding contexts near 1M tokens.

## Benchmark proof

Method: same client, same machine, same mock backend, direct call versus router call. Payloads are generated as coding-agent context: file paths, source snippets, diffs, logs, failing tests, and change instructions. This is closer to real vibe-coding traffic than repeated plain prose.

HTTP artifact: `benchmarks/artifacts/preview-3-http-1m-coding-context-summary.json` on Intel Xeon Gold 6148.

| Payload | Concurrency | p50 router overhead | p99 router overhead | Streaming first-byte delta | Router memory max |
|---|---:|---:|---:|---:|---:|
| `1k` | 50 | `+0.317 ms` | `+0.548 ms` | `-0.028 ms` | `60.04 MB` |
| `50k` | 50 | `+0.458 ms` | `+0.646 ms` | `-0.010 ms` | `60.04 MB` |
| `200k` | 50 | `+0.985 ms` | `+1.448 ms` | `-0.008 ms` | `60.04 MB` |
| `500k` | 50 | `+1.091 ms` | `+0.935 ms` | `-0.005 ms` | `60.04 MB` |
| `1m` | 50 | `+2.047 ms` | `+3.415 ms` | `-0.038 ms` | `60.04 MB` |
| `1m` | 200 | `+2.167 ms` | `+1.664 ms` | `-0.038 ms` | `60.04 MB` |

HTTPS artifact with local self-signed TLS enabled: `benchmarks/artifacts/preview-3-https-1m-coding-context-summary.json`.

| Payload | Concurrency | p50 router overhead | p99 router overhead | Streaming first-byte delta | Router memory max |
|---|---:|---:|---:|---:|---:|
| `1k` | 50 | `+0.357 ms` | `+96.932 ms` | `+21.251 ms` | `83.24 MB` |
| `50k` | 50 | `+0.830 ms` | `+101.449 ms` | `+19.469 ms` | `83.24 MB` |
| `200k` | 50 | `+1.972 ms` | `+68.994 ms` | `+20.504 ms` | `83.24 MB` |
| `500k` | 50 | `+3.555 ms` | `+31.512 ms` | `+22.184 ms` | `83.24 MB` |
| `1m` | 50 | `+7.008 ms` | `+12.055 ms` | `+21.760 ms` | `83.24 MB` |
| `1m` | 200 | `+6.539 ms` | `+35.845 ms` | `+21.760 ms` | `83.24 MB` |

The HTTPS streaming first-byte number includes the cost of a new local TLS connection in the sequential `curl` test. Production clients usually reuse connections, so this number should be read as a conservative new-connection measurement, not model streaming delay.

Both full runs had `0` router non-200 responses, `0` ledger drops, and passed the command-level smoke result. The HTTP run sustained `8,496.18 RPS` for the 1k saturation check and the HTTPS run sustained `8,496.80 RPS`. PostgreSQL ledger checks at 2,000 RPS observed at least 99% of expected rows during the measurement window, with no dropped ledger events.

## Quick start

Prerequisites: Linux, Docker, Docker Compose plugin, Git, and `curl`.

One-line install:

```bash
curl -fsSL https://raw.githubusercontent.com/thusinh1969/BrighTO_Router/main/install.sh | bash
```

The installer clones or updates the repo at `$HOME/brighto-router`, creates `.env` when missing, pulls the official Docker image, starts PostgreSQL, runs migrations, seeds defaults, and starts the router.

If you prefer to inspect the script first:

```bash
curl -fsSLO https://raw.githubusercontent.com/thusinh1969/BrighTO_Router/main/install.sh
less install.sh
bash install.sh
```

Manual install is still simple:

```bash
git clone https://github.com/thusinh1969/BrighTO_Router.git
cd BrighTO_Router
./start.sh install
```

This preview line pulls `thusinh1969/brighto_airouter:preview-3` by default. If you already have an old `.env`, make sure it contains `BRIGHTO_ROUTER_IMAGE=thusinh1969/brighto_airouter:preview-3`, then run `./start.sh restart`.

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

Open **Models & Routes → Add model route** in the Portal.

1. Choose **Task type**: Chat / LLM, Embedding, Rerank, or ASR / transcription.
2. Choose a provider preset such as OpenAI, Anthropic, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Jina AI, Voyage AI, Cohere, or **Custom LLM**.
3. Accept the default Base URL or enter your own.
4. Paste the provider API key when the endpoint requires one. Leave it blank when the matching `.env` key is already set, or when **Custom LLM** points to a local/no-auth endpoint such as llama.cpp.
5. Click **Load models** when the provider supports it, or use the task-specific suggestions/manual model name.
6. Select one provider model and set the public model name your apps will call.
7. Click **Test connection**.
8. Save the route only after the test passes.

The provider API key belongs to the model route. Client applications do not receive provider keys. They call BrighTO-Router with a client API key issued from the **API Keys** screen.

## First Model Group

Use a Model Group when you want one public OpenAI-compatible chat model name to spread traffic across several tested backend endpoints. The client does not change its API call. It still sends `model: "<public-group-name>"` to `/v1/chat/completions`.

Open **Models & Routes → Create Model Group** in the Portal.

1. Choose **OpenAI-compatible chat** as the group type. Preview-3 groups are for chat load balancing only.
2. Choose **Round robin** for equal rotation, or **Weighted round robin** when some endpoints should receive more traffic.
3. Add two or more endpoints with Base URL, provider model name, and provider API key/reference. For local/no-auth Custom LLM endpoints, leave the key blank.
4. Run **Test connection** for each endpoint.
5. Save the group enabled only after every endpoint you want active has passed.

Round robin ignores endpoint weights and rotates through available endpoints. Weighted round robin uses the configured weights consistently across router replicas through PostgreSQL counters. If one endpoint fails repeatedly, the router temporarily avoids it and uses the remaining healthy endpoints; later requests can probe it again so a recovered endpoint can rejoin service.

## Test a route from the command line

Use `test_router.py` as the simplest client example. It reads `.env` by default, so a fresh local install can use the seeded demo client key. Pass `--api-key` when testing with a key created in the Portal.

Text chat:

```bash
python3 test_router.py --model <public-model-name> --text "Reply OK in one short sentence."
```

Embeddings through an OpenAI-compatible embedding route:

```bash
python3 test_router.py --mode embeddings --model <public-embedding-route> --text "BrighTO embedding smoke test"
```

Rerank through a configured rerank route:

```bash
python3 test_router.py --mode rerank --model <public-rerank-route> --query "router speed" --document "fast Rust gateway" --document "slow proxy" --top-n 1
```

ASR / transcription through a configured multipart route:

```bash
python3 test_router.py --mode asr --model <public-asr-route> --file tests/fixtures/asr_smoke.wav
```

Live provider smoke tests for adapter keys and endpoints:

```bash
python3 scripts/adapter_smoke.py --provider qwen --task embedding
python3 scripts/adapter_smoke.py --provider qwen --task rerank
python3 scripts/adapter_smoke.py --provider jina --task embedding
python3 scripts/adapter_smoke.py --provider jina --task rerank
```

Provider shortcut tests through BrighTO-Router, using standard public route names such as `qwen-embedding`, `qwen-rerank`, `jina-embedding`, `jina-rerank`, `voyage-embedding`, `voyage-rerank`, and `cohere-rerank`:

```bash
python3 test_router.py --list-presets
python3 test_router.py --provider qwen --mode embeddings --text "hello"
python3 test_router.py --provider qwen --mode rerank --query "router speed"
python3 test_router.py --provider jina --mode embeddings --text "hello"
python3 test_router.py --provider jina --mode rerank --query "router speed"
python3 test_router.py --provider voyage --mode embeddings --text "hello"
python3 test_router.py --provider voyage --mode rerank --query "router speed"
python3 test_router.py --provider cohere --mode rerank --query "router speed"
```

Use `--model <your-public-route>` instead of `--provider` when your Portal route has a custom public name. In rerank mode, `--query` and `--text` both work; `--query` is clearer and takes priority.

Deterministic API matrix smoke with no paid provider keys. It starts temporary PostgreSQL, a temporary router, and the local mock upstream, then tests OpenAI-compatible chat, Anthropic Messages, embeddings, rerank, ASR multipart, and one protocol guard:

```bash
python3 scripts/api_matrix_smoke.py
```

Live router smoke through Admin API and public client endpoints, using whichever provider keys exist in `.env`. It covers OpenAI chat, OpenAI embeddings, OpenAI ASR, Qwen embeddings/rerank, Jina embeddings/rerank, Voyage embeddings/rerank, and Cohere rerank when the matching keys are present:

```bash
python3 scripts/adapter_router_smoke.py
```

Anthropic Messages live smoke is separate so teams can run it only when `ANTHROPIC_API_KEY` is available:

```bash
python3 scripts/anthropic_smoke.py
```

Preview-3 Model Group smoke examples:

```bash
./smoke/model_group/run_mock.sh
./smoke/model_group/run_live_openai_chat.sh
```

The mock smoke always runs locally and verifies weighted round-robin across three OpenAI-compatible chat endpoints. The live smoke creates one public OpenAI chat model group, `coding-fast-live`, backed by DeepSeek V4 Pro and local llama.cpp `qwen3.8-flash-next`. It requires `DEEPSEEK_API_KEY` and a reachable local llama.cpp OpenAI-compatible endpoint, defaulting to `http://127.0.0.1:8088/v1`.

Image input through an OpenAI-style multimodal chat route:

```bash
python3 test_router.py --model <vision-model-route> --text "Describe this image." --image ./photo.jpg
```

Audio input through an OpenAI-style multimodal chat route:

```bash
python3 test_router.py --model <audio-model-route> --text "Summarize this audio." --audio tests/fixtures/asr_smoke.wav
```

For HTTPS with a self-signed certificate, add `--insecure`. The image and audio examples are JSON pass-through examples; the selected backend model must support that payload shape.

## What install creates

`./start.sh install` creates `.env` from `.env.example`, starts PostgreSQL in Docker, runs migrations, seeds default records, pulls `thusinh1969/brighto_airouter:preview-3`, and starts the router.

Default records:

| Record | Created value | Purpose |
|---|---|---|
| Team | `Default Team` | Lets an admin create client API keys immediately. |
| Demo client key | `sk-brighto-0123456789abcdef0123456789abcdef` | Local smoke testing only. Replace or disable it before shared use. |
| Model routes | None | You choose which provider models clients can call. |
| Provider endpoints | OpenAI, Anthropic, Gemini, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Meta Muse, Custom LLM, Jina AI, Voyage AI, Cohere, Qwen Rerank | Friendly defaults for the Portal. They are endpoint templates, not usable routes until a tested model route is saved. |
| Provider catalog | `PROVIDER_CATALOG` in `.env` | Controls the Add model route provider dropdown. |

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

### Optional Kubernetes scale-out

Most teams should start with Docker Compose. The Rust router is fast enough that one well-sized node can handle serious traffic while staying simple to operate. Kubernetes is optional for teams that already run Kubernetes or need operational scale-out.

If `kubectl` already points to a single-node or multi-node cluster, BrighTO-Router can install router replicas with one command:

```bash
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router' --k8s --replicas 3
```

Use an external or managed PostgreSQL database for any real multi-node deployment. The included `k8s/postgres.dev.yaml` is only a local development starter and uses temporary pod storage. The router pods are stateless; they reload config from PostgreSQL, enforce policy from an in-memory snapshot, and can sit behind your Kubernetes Service, Ingress, or load balancer.

Why use Kubernetes if the router is already very fast? Availability and operations: multiple pods survive one pod/node restart, rolling upgrades avoid planned downtime, long streams from many developers can be spread across pods, and traffic can grow without changing the application endpoint.

More detail: [INSTALL.md](INSTALL.md), [HTTPS.md](HTTPS.md), [PROVIDERS.md](PROVIDERS.md), [k8s/README.md](k8s/README.md).

## What teams get in preview-3

- One internal endpoint for multiple model providers.
- OpenAI-style routes: `/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, `/v1/models`.
- Anthropic Messages route: `/v1/messages`.
- Adapter routes: `/v1/rerank` and `/v1/audio/transcriptions` are implemented, Portal task-aware, mock/integration tested, and live-smoked with OpenAI, Qwen/DashScope, Jina, Voyage, and Cohere.
- Multimodal LLM JSON pass-through when the selected backend supports that request shape.
- Model aliases and provider-backed model routes.
- Weighted backend routing, fallback backend support, and circuit breaking.
- Team budgets and API-key budgets.
- API-key expiry, request-per-minute limits, and concurrency limits.
- PostgreSQL usage ledger with file fallback if PostgreSQL is temporarily unavailable.
- Admin Portal for providers, model routes, teams, API keys, usage, and budgets.
- User Portal for issued client keys.
- Health endpoints: `/healthz`, `/readyz`.
- Prometheus metrics endpoint: `/metrics`.

## Logging, analytics, and privacy

BrighTO-Router logs one usage record per API call. It does not store chat content, prompts, uploaded media, tool payloads, or model responses. In 1.0-preview-3, a "session" in the router means request-level traffic metadata, not a stored conversation transcript.

Usage records are written to PostgreSQL in `usage_ledger`. If PostgreSQL is temporarily unavailable, the router writes usage events to the local JSONL file configured by `LEDGER_FALLBACK_FILE` (`/var/lib/brighto-router/ledger-fallback.jsonl` in the default Docker setup) and replays them when the database is available again. PostgreSQL is the source for Portal reporting, budget counters, historical analytics, and Grafana SQL dashboards. The fallback file is only a durability buffer during database outages.

| Customer question | preview-3 answer | Why it matters |
|---|---|---|
| Do we log request size? | Yes, by `input_tokens`, `output_tokens`, and an `estimated` flag when the provider did not return exact usage. | Enough for budget, cost, and capacity analysis without storing content. |
| Do we log speed? | Yes: `ttfb_ms` (time to first byte), `total_ms` (whole request), and `router_overhead_ms` (router work before provider forwarding). Token-per-second values are derived from token counts and duration. | Admins can see whether latency comes from the provider, large payloads, or router overhead. |
| Do we log errors? | Yes: HTTP `status`, `client_aborted`, and a short `error_class` such as timeout, read failure, network error, or client aborted. | Supports error-rate dashboards and operational alerts. |
| Do we log provider error text/body? | No. The router forwards provider errors to the caller but does not persist the provider response body. | Provider error bodies can contain prompt fragments, account details, or sensitive payload context. |
| Do we keep chat content? | No. No prompt, message array, image/audio payload, tool call body, or model answer is stored by the router. | Keeps the hot path fast, reduces storage cost, and avoids turning the router into a private data lake. |
| Can Grafana use the data? | Yes. Grafana can read PostgreSQL `usage_ledger` for history and `/metrics` for Prometheus time-series metrics. | Teams get both business analytics and live infrastructure metrics. |

The Portal already uses the same ledger data for totals by provider, model, team, API key, prompt-size bucket, latency, token throughput, and errors. Prometheus `/metrics` exposes router counters, token counters, first-byte latency, router overhead, and ledger health metrics for Grafana or alerting.

If a customer needs full transcript auditing, that should be an explicit enterprise feature with separate retention policy, encryption, redaction, and access controls. It should not be enabled silently in the router core. The current default is privacy-preserving metadata logging.

## Multimodal and media support

BrighTO-Router preview-3 routes LLM, embeddings, rerank, and OpenAI-compatible ASR/transcription requests. It does not try to be a full media-generation gateway yet. The router authenticates the client, checks policy, chooses the configured model route, and forwards the JSON body to the selected backend. It does not inspect, transform, store, resize, transcode, or normalize media content.

| Capability | preview-3 status | What it means |
|---|---|---|
| Text chat/completions | Yes | Supported through OpenAI-style `/v1/chat/completions` and `/v1/completions`. |
| Embeddings | Proxy yes | Supported through OpenAI-style `/v1/embeddings` when the backend provides embeddings. BrighTO-Router forwards the request and returns the vector response unchanged. |
| Anthropic Messages | Yes | Supported through `/v1/messages` for Anthropic-compatible backends. |
| Image input inside LLM chat JSON | Conditional yes | Passed through when the selected backend accepts that JSON shape and the request stays under `MAX_BODY_BYTES`. |
| Audio input inside LLM chat JSON | Conditional yes | Passed through only when the backend accepts audio data in the same JSON endpoint. This is separate from the multipart ASR adapter below. |
| Video input inside LLM chat JSON | Conditional yes | Passed through only when the backend accepts video data in the same JSON endpoint and the body-size limit allows it. |
| OpenAI Images API such as `/v1/images/generations` | No | Planned as a future media adapter, not part of preview-3. |
| Audio generation / TTS | No | Planned as future media adapters. Preview-3 supports ASR/transcription only for OpenAI-compatible multipart providers. |
| Video generation routes | No | Planned as future media adapters, not part of preview-3. |
| Reranking APIs | Yes | `/v1/rerank` supports Jina, Voyage, Cohere, Qwen/DashScope, and OpenAI-compatible/custom rerank adapters. |
| Multipart ASR upload | Yes | `/v1/audio/transcriptions` supports OpenAI-compatible transcription providers and has live OpenAI smoke coverage. |
| Realtime voice or WebSocket media sessions | No | Future enterprise/media work if customer demand requires it. |

The practical rule is simple: if a provider exposes a model through a supported JSON LLM endpoint, BrighTO-Router can route it. If the provider needs a separate image/audio/video/rerank API, multipart upload flow, realtime session, or provider-specific media protocol, that belongs in a future adapter.

### Embeddings and reranking scope

`/v1/embeddings` is a proxy route, not an embedding engine. The backend creates the vector. BrighTO-Router only applies authentication, model-route policy, budget checks, provider credential handling, response forwarding, and usage logging. It does not store vectors, build a vector index, run semantic search, or convert one provider's embedding format into another.

BGE or Qwen text embedding models can be routed when they are exposed by an OpenAI-compatible backend that accepts `/v1/embeddings`; preview-3 live-smokes Qwen `qwen3.7-text-embedding` this way. Qwen `tongyi-embedding-vision-flash` is a multimodal embedding model, but it uses DashScope multimodal embedding APIs and should be handled by a future dedicated adapter. In preview-3, reranking is implemented as a separate adapter endpoint because reranking has a different request and response shape from embeddings.

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
| Ledger fallback | Local JSONL file from `LEDGER_FALLBACK_FILE` | Keeps serving during a temporary PostgreSQL outage. |

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
| `500k` | 1, 50, 200 | Correctness, memory, overhead, first byte | Extreme coding contexts should stay pass-through without memory growth. |
| `1m` | 1, 50, 200 | Correctness, memory, overhead, first byte | Vibe-coding and repository-analysis contexts near 1M tokens must stay stable. |

Current verified public-facing status:

- The large-context proof artifacts cover the preview-3 fast path from `1k` through `1m`, at concurrency 1, 50, and 200. Model Group logic has separate integration and browser smoke coverage.
- The headline 1M HTTP result at concurrency 200 is `+2.167 ms p50` and `+1.664 ms p99` router overhead with `0` non-200 responses.
- The headline 1M HTTPS result at concurrency 200 is `+6.539 ms p50` and `+35.845 ms p99` router overhead with `0` non-200 responses.
- Hard release thresholds still apply to the calibrated `1k`, `50k`, and `200k` gates. The `500k` and `1m` artifacts are published measurement proof and will become hard gates only after we have more repeated public baselines.
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

The first enterprise priority is a production Kubernetes implementation for very large deployments. The core architecture is already designed for that path: router instances are stateless, configuration is reloaded from PostgreSQL, and traffic can be spread across many pods behind a load balancer. With a properly sized Kubernetes cluster, managed or highly available PostgreSQL, provider capacity planning, and standard observability, the same architecture can scale toward serving millions of customers without a major rewrite.

Enterprise work will focus on packaging and operating that architecture professionally for teams serving very large traffic:

- Production Kubernetes manifests and Helm-style configuration.
- Horizontal router scaling across many pods.
- PostgreSQL high-availability guidance or managed PostgreSQL integration.
- Rolling upgrades with zero planned downtime.
- SSO: Single Sign-On through OIDC or SAML.
- RBAC: role-based admin permissions.
- Organization and project hierarchy.
- Approval workflow for provider/model changes.
- Central audit log export.
- Secrets manager integration.
- Multi-region deployment guidance.
- Support packages and performance certification on customer hardware.

Billing should be an enterprise adapter beside the router, not code inside the fastest request path. The open-source router already records durable usage in PostgreSQL. An enterprise billing adapter can read that ledger and connect it to Stripe, Chargebee, an internal billing system, prepaid credits, monthly invoices, departmental chargeback, and customer-specific pricing. Keeping billing outside the hot path protects latency and keeps the open-source core simple.

Dedicated media APIs should also be future adapters, not hidden preview promises. Possible enterprise or later open-source extensions include image generation routes, audio generation, Text-to-Speech, F5-TTS-compatible endpoints, video generation, more provider-specific ASR adapters, larger multipart policies, and realtime voice sessions. Those adapters should plug into the same auth, budget, team, ledger, and Portal model without making the core LLM router harder to operate.

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

BrighTO-Router is released under the Apache License 2.0. See [LICENSE](LICENSE).

---

Copyright © 2026 **Nguyễn Anh Nguyên**, **BrighTO AI**.

Created and maintained by Nguyễn Anh Nguyên. Contact: `nguyen@hatto.com`.
