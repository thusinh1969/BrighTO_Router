# BrighTO-Router — Rust LLM Gateway, Model Load Balancer & SystemOne Router

**Million-token AI traffic, simple Rust fast path, one Docker install.**

BrighTO-Router is an ultra-fast open-source, self-hosted LLM gateway, AI router, model load balancer, and SystemOne decision router written in Rust. It gives a team one stable API endpoint for OpenAI-compatible, Anthropic-compatible, cloud, local, Ollaya/Laya, and JEV/DJEV-style System One backends; keeps provider keys private; records usage in PostgreSQL; and adds NGINX-style Model Groups for **round-robin or weighted load balancing**. Also searchable as **Brighto LLM Router**, it fits teams looking for an LLM API proxy with fallback routing, token budgets, usage analytics, decision routing, and cost-control infrastructure they can own.

Use it as a free, open-source LiteLLM or Bifrost alternative when you want a narrow, fast, self-hosted traffic path instead of a broad hosted AI platform.

<p align="center">
  <img src="docs/assets/brighto-router-architecture.png" alt="BrighTO-Router architecture: open-source Rust LLM gateway with model load balancing, chat completions, embeddings, rerank, ASR transcription, PostgreSQL usage metadata, and privacy-first no prompt storage" />
</p>


Quick menu: [Install](#quick-start) · [First route](#first-model-route) · [Python SDK](#single-file-python-sdk) · [API examples](docs/API_EXAMPLES.md) · [Agent clients](docs/AGENT_CLIENTS.md) · [System One](#system-one--decision-routes) · [Model Groups](#first-model-group) · [Architecture](#how-it-works) · [Benchmarks](#benchmark-strategy) · [API support](#multimodal-and-media-support) · [Operations](#daily-operation) · [Privacy](#logging-analytics-and-privacy)

- Official repository: `https://github.com/thusinh1969/BrighTO_Router`
- Official Docker image: `thusinh1969/brighto_airouter:v1.1.0` (`linux/amd64` and `linux/arm64`)
- Search keywords: open-source LLM gateway, free LLM router, LLM gateway, LLM router, AI gateway, model router, Rust LLM proxy, OpenAI-compatible gateway, Anthropic-compatible router, SystemOne router, System One decisions, JEV router, DJEV router, Ollaya router, Laya model, LiteLLM alternative
- Release version: `1.1.0`

## Why teams choose BrighTO-Router

| Strength | What it means |
|---|---|
| **Ultra-fast large-context routing** | Same-machine mock benchmarks show million-token pass-through overhead in low single-digit milliseconds over HTTP. |
| **Dead-simple production stack** | One Rust binary, one Docker image, PostgreSQL as the durable store. No Redis required for 1.1.0. |
| **Multi-core by default** | The router uses all available CPU threads by default; set `ROUTER_WORKER_THREADS` only when you need to cap CPU use. |
| **Load balancing built in** | Model Groups let one API model name spread traffic across compatible routes using round-robin or weighted round-robin. |
| **Easy to run and maintain** | `./start.sh install`, `start`, `stop`, `status`, `logs`, `restart`; Portal for routes, teams, keys, budgets, and usage. |
| **Private by design** | The router records metadata for usage analytics, not prompt text, uploaded media, tool payloads, or model answers. |
| **Self-hosted control** | Works with OpenAI-compatible providers, Anthropic Messages, local llama.cpp/vLLM-style endpoints, embeddings, rerank, and ASR routes. |

Compared with broad AI gateway platforms, BrighTO-Router keeps the promise narrower: be the fast, understandable LLM gateway a team can own. It is not a hosted prompt suite or agent platform. It is the traffic path, policy point, model load balancer, and usage ledger for your AI endpoints.

## Benchmark headline

These benchmarks use deterministic local Rust mock backends. They measure router overhead, not model inference speed, and they do not call paid cloud providers.

| Scenario | Result | Artifact |
|---|---:|---|
| 1M-token HTTP pass-through, 200 concurrent | `+2.167 ms p50`, `+1.664 ms p99` overhead, `0` non-200 | `benchmarks/artifacts/v1-http-1m-coding-context-summary.json` |
| 1M-token HTTPS pass-through, 200 concurrent | `+6.539 ms p50`, `+35.845 ms p99` overhead, `0` non-200 | `benchmarks/artifacts/v1-https-1m-coding-context-summary.json` |
| 1k Model Group, 200 concurrent, round-robin | `+0.329 ms p50` overhead, ~`1,999 RPS`, `0` non-200 | `benchmarks/artifacts/v1-model-group-lb-current-summary.json` |
| 1k Model Group, 200 concurrent, weighted | `+0.364 ms p50` overhead, ~`1,999 RPS`, `0` non-200 | `benchmarks/artifacts/v1-model-group-lb-current-summary.json` |
| 500k Model Group, 200 concurrent, round-robin | `+4.876 ms p50` overhead, ~`80 RPS`, `0` non-200 | `benchmarks/artifacts/v1-model-group-lb-current-summary.json` |
| 500k Model Group, 200 concurrent, weighted | `+4.446 ms p50` overhead, ~`80 RPS`, `0` non-200 | `benchmarks/artifacts/v1-model-group-lb-current-summary.json` |
| 1M Model Group, 200 concurrent, round-robin | `+9.739 ms p50` overhead, ~`40 RPS`, `0` non-200 | `benchmarks/artifacts/v1-model-group-lb-current-summary.json` |
| 1M Model Group, 200 concurrent, weighted | `+10.679 ms p50` overhead, ~`40 RPS`, `0` non-200 | `benchmarks/artifacts/v1-model-group-lb-current-summary.json` |

Honest read: Model Groups add routing choice, per-endpoint model rewrite, PostgreSQL-backed round-robin counters, and fail-safe behavior. The table reports standalone Model Group overhead against a direct mock backend. We do not claim a negative Model Group delta versus single route; release validation uses a fair same-mock gate so impossible-looking benchmark wins cannot slip into public claims.

## Core capabilities

| Capability | Current 1.1.0 status |
|---|---|
| Chat, completions, and Responses | OpenAI-compatible `/v1/chat/completions`, `/v1/completions`, and `/v1/responses`. |
| Anthropic Messages | `/v1/messages` with Anthropic-compatible upstreams. |
| Embeddings | `/v1/embeddings` pass-through with usage logging. |
| Rerank | `/v1/rerank` adapters for Qwen/DashScope, Jina, Voyage, Cohere, and OpenAI-compatible/custom endpoints. |
| ASR / transcription | `/v1/audio/transcriptions` multipart proxy path. |
| System One / decisions | `/v1/systemone` and `/v1/decisions` for TypeSafe/Jev-compatible decision models such as Ollaya/Laya or hosted Jev-style endpoints. |
| Model Groups | Same-type routes behind one API model name; round-robin or weighted round-robin. |
| Fail-safe endpoint handling | A failed endpoint is skipped after repeated pre-response failures and retried after `BACKEND_CIRCUIT_OPEN_SECONDS`, default `30`. |
| Teams, keys, budgets | Team budgets, visible client API keys, expiry, request-per-minute limits, concurrency limits, and usage dashboard. |

## Real workloads this targets

| Workload | Why it fits |
|---|---|
| Many team users with small and medium prompts | Fast Rust hot path: authenticate, check policy, choose route, forward stream, write usage asynchronously. |
| Vibe-coding and agent traffic with huge contexts | Large JSON bodies stay pass-through; the router avoids storing prompt content and keeps memory behavior visible. |
| Multiple local and cloud backends | Model Groups can balance one client-facing model name across compatible endpoints while clients keep the same request. |

## Agent clients

Claude Code, Codex, Hermes, and OpenClaw can use the router directly with a BrighTO client key and a matching native API route. Choose **Anthropic Messages** for Claude Code, **OpenAI Responses** for Codex, or **Chat** for Hermes and OpenClaw. Streaming and tool payloads are forwarded; the upstream must support that protocol and tool calling.

[Client configuration and HTTPS trust](docs/AGENT_CLIENTS.md) · [Repeatable native CLI tool tests](smoke/agents/README.md)

## Quick start

Prerequisites: Docker Engine or Docker Desktop with the Compose plugin, Bash, Python 3.9+, Git, and `curl`. Python is used by the install/client scripts; the router runs as a Rust binary. Docker Desktop users must enable host networking; Windows users run the installer inside WSL2. See [platform support](INSTALL.md#supported-install-environments).

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

This 1.1.0 line pulls `thusinh1969/brighto_airouter:v1.1.0` by default. Docker automatically selects the matching `linux/amd64` or `linux/arm64` image. If you already have an old `.env`, make sure it contains `BRIGHTO_ROUTER_IMAGE=thusinh1969/brighto_airouter:v1.1.0`, then run `./start.sh restart`.

Open the Portal on the server:

```text
http://127.0.0.1:18080/
```

Open it from another machine:

```text
http://<SERVER_IP>:18080/
```

Admin login uses username `admin` and the random `ADMIN_MASTER_KEY` generated in `.env` during install. Keep `.env` private.

### Admin access CIDR: install works first, then lock it down

`ADMIN_ALLOW_CIDR` is the list of IP addresses or IP ranges allowed to use the Admin Portal and Admin API. **CIDR** means IP range notation, for example `203.0.113.10/32` for one public IP, `10.0.0.0/8` for a private network, or `0.0.0.0/0` for all IPv4 addresses.

Fresh installs default to:

```bash
ADMIN_ALLOW_CIDR=0.0.0.0/0,::/0
```

That is intentional: the generated admin key must work from the Portal URL printed by the installer, including a browser on another machine. The admin key is still required. After first login, production servers should restrict it to your office, VPN, bastion host, or reverse-proxy range:

```bash
# edit .env
ADMIN_ALLOW_CIDR=<YOUR_PUBLIC_IP>/32

./start.sh restart
```

If the Portal says `Admin access blocked: ip not allowed`, your browser IP is outside `ADMIN_ALLOW_CIDR`. Set it to a range that includes your browser, or temporarily use `0.0.0.0/0,::/0`, then restart.

HTTPS first-run with a local self-signed certificate:

```bash
./start.sh install --https --host <SERVER_HOST_OR_IP>
```

Check the stack at any time:

```bash
./start.sh status
```

`status` prints the active Portal URL, local health checks, and the command to test from another machine.

## First model route

Open **Models & Routes → Add model route** in the Portal.

1. Choose **Task type**. The Portal shows the endpoint shape explicitly: **Chat Completions (`/v1/chat/completions`)**, **Completions (`/v1/completions`)**, **Responses API (`/v1/responses`)**, Embedding, Rerank, ASR / transcription, or System One / Decision.
2. Choose a provider preset such as OpenAI, Anthropic, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Jina AI, Voyage AI, Cohere, **Ollaya System One**, or **Custom LLM**.
3. Accept the default Base URL or enter your own.
4. Paste the provider API key when the endpoint requires one. Leave it blank when the matching `.env` key is already set, or when **Custom LLM** points to a local/no-auth endpoint such as llama.cpp or local Ollaya.
5. Click **Load models** when the provider supports it, or use the task-specific suggestions/manual model name.
6. Select one provider model and set the API model name your apps will call.
7. Click **Test connection**.
8. Save the route only after the test passes.

The provider API key belongs to the model route. Client applications do not receive provider keys. They call BrighTO-Router with a client API key issued from the **API Keys** screen.

## Single-file Python SDK

Copy [brighto.py](brighto.py) next to your application. It needs Python 3.9+ and one dependency:

```bash
python3 -m pip install requests
export BRIGHTO_API_KEY="<client-api-key-from-the-portal>"
export BRIGHTO_ROUTER_URL="http://127.0.0.1:18080"
```

```python
from brighto import Router

with Router(model="<public-chat-route-or-group>") as client:
    print(client.ask("Reply OK in one short sentence."))
```

Use the **API model name** from the Portal, including a Model Group name when you want load balancing. The URL may be the router root or end in `/v1`; the SDK handles the prefix. Your BrighTO client key authenticates the application; provider keys stay on the router.

The same file covers chat and streaming, legacy completions, Responses, Anthropic Messages, embeddings, rerank, audio transcription, and System One decisions. [Python examples for every type](docs/API_EXAMPLES.md) show the matching method, return value, HTTPS configuration, multimodal inputs, and error handling. `test_router.py` uses this SDK too.

## System One / Decision routes

BrighTO-Router routes TypeSafe/Jev-compatible decision requests through `/v1/systemone` or `/v1/decisions`. Backends can include Ollaya/Laya, hosted Jev-compatible services, or [Quyết by Chinh Nguyen](https://github.com/ncchinh/quyet), served through its compatible decision interface.

For a tested System One route or Model Group named `quyet-small`:

```python
from brighto import Router

with Router(model="quyet-small") as client:
    result = client.systemone(
        state={"message": "I was charged twice for one order."},
        questions={
            "duplicate_charge": {
                "type": "noul",
                "instructions": "The customer reports a duplicate charge.",
            },
            "team": {
                "type": "choice",
                "instructions": "Which team should handle this?",
                "criteria": {"billing": "payments and refunds", "support": "technical help"},
            },
        },
    )
    print(result["answers"])
```

`noul` returns the probability that a statement is true; `choice` selects from named options. `client.decisions(...)` sends the same contract through `/v1/decisions`. The [Quyết setup example](docs/API_EXAMPLES.md#quyet-system-one-backend) explains how to serve its Python runtime and connect it to the router.

To self-test with Ollaya and Laya locally, run:

```bash
./smoke/systemone/run_ollaya_laya.sh
```

That script starts an Ollaya Docker container, pulls `laya`, creates BrighTO System One routes and a Model Group, calls `/v1/systemone` and `/v1/decisions`, waits past the backend health interval, then calls again. To test an auth-required Ollaya/Jev-like backend, set `OLLAYA_API_KEY`:

```bash
OLLAYA_API_KEY="test-systemone-key" ./smoke/systemone/run_ollaya_laya.sh
```

## First Model Group

API model names are unique across normal model routes and Model Groups. This is the value clients send in JSON as `model`; a future display label can be decorative, but this API name must not collide.

Use a Model Group when you want one API model name to spread traffic across several existing tested routes of the same API shape. The client does not change its API call. It still sends `model: "<api-model-group-name>"` to the same endpoint for that type. Chat Completions, Completions, and Responses are separate shapes and are not mixed in one group.

If an endpoint returns a retryable failure before a response is committed, the router tries another healthy endpoint in the group. After three consecutive failures, that backend opens a circuit and is checked again after `BACKEND_CIRCUIT_OPEN_SECONDS` seconds. The default is `30`; set it in `.env` when you need faster or slower recovery probes.

Open **Models & Routes → Create Model Group** in the Portal.

1. Choose the **Model type**: Chat / LLM, Embedding, Rerank, ASR / transcription, or System One / Decision.
2. Choose **Round robin** for equal rotation, or **Weighted round robin** when some routes should receive more traffic.
3. Add two or more existing tested routes. The Portal only lists routes that match the selected type.
4. Save the group enabled. No provider URL or provider API key is entered in the group wizard; those belong to the source routes.

Round robin rotates evenly and does not ask for weights. Weighted round robin shows per-route weights and uses them consistently across router replicas through PostgreSQL counters. If one endpoint fails repeatedly, the router temporarily avoids it and uses the remaining healthy endpoints; later requests can probe it again so a recovered endpoint can rejoin service.

## Test a route from the command line

Keep [test_router.py](test_router.py) and [brighto.py](brighto.py) together and install `requests`. The helper reads `.env` for local installation defaults and reports HTTP status, elapsed time, and the router request ID.

```bash
python3 -m pip install requests
python3 test_router.py --model <public-chat-route-or-group> --text "Reply OK"
python3 test_router.py --model <public-chat-route-or-group> --text "Reply OK" --stream
python3 test_router.py --mode rerank --model <public-rerank-route> --query "router speed" --document "fast Rust gateway" --document "unrelated text" --top-n 1
```

Choose `--mode chat`, `completions`, `responses`, `messages`, `embeddings`, `rerank`, `asr`, or `systemone` to match the route's Portal task. Model Groups use the same mode as their member routes. [All SDK and command-line examples](docs/API_EXAMPLES.md) include embeddings, audio uploads, decisions, and multimodal chat.

For HTTPS with your own certificate:

```bash
python3 test_router.py --router https://<router-host>:18443/v1 --ca ssl/cert.pem --model <public-chat-route> --text "Reply OK"
```

`--insecure` is available for local self-signed testing. `--dry-run` shows the request without sending it, and `--raw-response` prints complete JSON. Provider shortcuts remain available through `--list-presets`; custom names always use `--model`.

## What install creates

`./start.sh install` creates `.env` from `.env.example`, starts PostgreSQL in Docker, runs migrations, seeds default records, pulls `thusinh1969/brighto_airouter:v1.1.0`, and starts the router.

Default records:

| Record | Created value | Purpose |
|---|---|---|
| Team | `Default Team` | Lets an admin create client API keys immediately. |
| Demo client key | Random `sk-brighto-...` in `.env` | Local smoke testing only. Rotate, disable, or delete it before shared use. |
| Model routes | None | You choose which provider models clients can call. |
| Provider endpoints | OpenAI, Anthropic, Gemini, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Meta Muse, Custom LLM, Ollaya System One, Jina AI, Voyage AI, Cohere, Qwen Rerank | Friendly defaults for the Portal. They are endpoint templates, not usable routes until a tested model route is saved. |
| Provider catalog | `PROVIDER_CATALOG` in `.env` | Controls the Add model route provider dropdown. |

`./start.sh start`, `./start.sh restart`, Docker image pulls, and Docker image rebuilds do **not** wipe PostgreSQL. Local data is stored in the Docker named volume `brighto-airouter_pg-data`. Data is removed only when you explicitly delete the volume, run `docker compose down -v`, or manually reset the database.

## Upgrade without export/import pain

Normal upgrades are in-place. Users do **not** need to export routes, reinstall, and import them again.

```bash
cd ~/brighto-router
./start.sh upgrade
```

That command creates a private backup under `backups/`, pulls the configured Docker image, runs SQL migrations, seeds any missing default templates, recreates only the router container, and keeps the existing PostgreSQL data.

Upgrade also protects existing users from the old LAN-only admin default. If `.env` still has the old default `127.0.0.1/32,::1/128,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16`, `./start.sh upgrade` changes it to `0.0.0.0/0,::/0` so the Portal URL printed by `./start.sh status` can log in immediately. If you already set a custom `ADMIN_ALLOW_CIDR`, upgrade leaves it untouched.

To pin a new image tag explicitly:

```bash
./start.sh upgrade --image thusinh1969/brighto_airouter:v1.1.0
```

Manual safety commands are available when moving servers or before a risky maintenance window:

```bash
./start.sh backup
./start.sh restore backups/brighto-backup-YYYYMMDD-HHMMSS --yes
```

`restore` replaces the current database, so it requires `--yes`. Add `--with-env` only when you intentionally want to restore the saved `.env` credentials too.

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

Useful environment knobs:

| Variable | Default | Meaning |
|---|---:|---|
| `CONFIG_POLL_SECS` | `5` | How often router replicas reload PostgreSQL config snapshots. |
| `BACKEND_CIRCUIT_OPEN_SECONDS` | `30` | How long a failed backend stays out before a half-open retry. |
| `MAX_BODY_BYTES` | `67108864` | Maximum accepted request body size. |
| `ROUTER_WORKER_THREADS` | all available CPU threads | Optional cap for Tokio worker threads. Leave blank for production throughput. |

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

## What teams get in 1.1.0

- One internal endpoint for multiple model providers.
- OpenAI-style routes: `/v1/chat/completions`, `/v1/completions`, `/v1/responses`, `/v1/embeddings`, `/v1/models`.
- Anthropic Messages route: `/v1/messages`.
- Adapter routes: `/v1/rerank`, `/v1/audio/transcriptions`, `/v1/systemone`, and `/v1/decisions` are implemented, Portal task-aware, mock/integration tested, and live-smoked with OpenAI, Qwen/DashScope, Jina, Voyage, Cohere, and Ollaya/Laya.
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

BrighTO-Router logs one usage record per API call. It does not store chat content, prompts, uploaded media, tool payloads, or model responses. In 1.1.0, a "session" in the router means request-level traffic metadata, not a stored conversation transcript.

Usage records are written to PostgreSQL in `usage_ledger`. If PostgreSQL is temporarily unavailable, the router writes usage events to the local JSONL file configured by `LEDGER_FALLBACK_FILE` (`/var/lib/brighto-router/ledger-fallback.jsonl` in the default Docker setup) and replays them when the database is available again. PostgreSQL is the source for Portal reporting, budget counters, historical analytics, and Grafana SQL dashboards. The fallback file is only a durability buffer during database outages.

| Customer question | 1.1.0 answer | Why it matters |
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

BrighTO-Router 1.1.0 routes LLM, OpenAI Responses, embeddings, rerank, System One decisions, and OpenAI-compatible ASR/transcription requests. It does not try to be a full media-generation gateway yet. The router authenticates the client, checks policy, chooses the configured model route, and forwards the JSON body to the selected backend. It does not inspect, transform, store, resize, transcode, or normalize media content.

| Capability | 1.1.0 status | What it means |
|---|---|---|
| Text chat/completions | Yes | Supported through OpenAI-style `/v1/chat/completions` and `/v1/completions`. |
| OpenAI Responses | Proxy yes | Supported through `/v1/responses` when the backend exposes the Responses API. Useful for Codex-style clients. |
| Embeddings | Proxy yes | Supported through OpenAI-style `/v1/embeddings` when the backend provides embeddings. BrighTO-Router forwards the request and returns the vector response unchanged. |
| Anthropic Messages | Yes | Supported through `/v1/messages` for Anthropic-compatible backends. |
| Image input inside LLM chat JSON | Conditional yes | Passed through when the selected backend accepts that JSON shape and the request stays under `MAX_BODY_BYTES`. |
| Audio input inside LLM chat JSON | Conditional yes | Passed through only when the backend accepts audio data in the same JSON endpoint. This is separate from the multipart ASR adapter below. |
| Video input inside LLM chat JSON | Conditional yes | Passed through only when the backend accepts video data in the same JSON endpoint and the body-size limit allows it. |
| OpenAI Images API such as `/v1/images/generations` | No | Planned as a future media adapter, not part of 1.1.0. |
| Audio generation / TTS | No | Planned as future media adapters. 1.1.0 supports ASR/transcription only for OpenAI-compatible multipart providers. |
| Video generation routes | No | Planned as future media adapters, not part of 1.1.0. |
| Reranking APIs | Yes | `/v1/rerank` supports Jina, Voyage, Cohere, Qwen/DashScope, and OpenAI-compatible/custom rerank adapters. |
| Multipart ASR upload | Yes | `/v1/audio/transcriptions` supports OpenAI-compatible transcription providers and has live OpenAI smoke coverage. |
| System One / decisions | Yes | `/v1/systemone` and `/v1/decisions` forward TypeSafe/Jev-compatible decision requests and return answers unchanged. |
| Realtime voice or WebSocket media sessions | No | Future enterprise/media work if customer demand requires it. |

The practical rule is simple: if a provider exposes a model through a supported JSON LLM endpoint or the System One decision contract, BrighTO-Router can route it. If the provider needs a separate image/audio/video/rerank API, multipart upload flow, realtime session, or provider-specific media protocol, that belongs in a future adapter.

### Embeddings and reranking scope

`/v1/responses` is a proxy route, not an agent runtime. The backend implements Responses semantics. BrighTO-Router authenticates, selects the route, forwards the JSON body, returns the provider response unchanged, and logs usage metadata when usage is present.

`/v1/embeddings` is a proxy route, not an embedding engine. The backend creates the vector. BrighTO-Router only applies authentication, model-route policy, budget checks, provider credential handling, response forwarding, and usage logging. It does not store vectors, build a vector index, run semantic search, or convert one provider's embedding format into another.

BGE or Qwen text embedding models can be routed when they are exposed by an OpenAI-compatible backend that accepts `/v1/embeddings`; 1.1.0 live-smokes Qwen `qwen3.7-text-embedding` this way. Qwen `tongyi-embedding-vision-flash` is a multimodal embedding model, but it uses DashScope multimodal embedding APIs and should be handled by a future dedicated adapter. In 1.1.0, reranking is implemented as a separate adapter endpoint because reranking has a different request and response shape from embeddings.

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
| Model Group counters | PostgreSQL | Keeps round-robin allocation consistent across router replicas. |
| Usage ledger | PostgreSQL | Durable cost and usage record. |
| Ledger fallback | Local JSONL file from `LEDGER_FALLBACK_FILE` | Keeps serving during a temporary PostgreSQL outage. |
| Backend recovery probe | `BACKEND_CIRCUIT_OPEN_SECONDS`, default `30` | How long a failed backend stays out of rotation before a half-open test request can let it rejoin. |

PostgreSQL is intentionally narrow: durable control plane, cross-pod Model Group counters, and usage metadata. It is not in the per-token provider streaming path. See the interactive diagrams: [v1.1.0 architecture](docs/architecture/brighto-router-workflow.html) and [PostgreSQL data flow](docs/postgresql-data-flow.html).

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

- The large-context proof artifacts cover the 1.0/1.1 fast path from `1k` through `1m`, at concurrency 1, 50, and 200.
- The current Model Group load-balancing artifact covers two local mock endpoints, round-robin and weighted round-robin, payloads `1k`, `500k`, and `1m` at concurrency 200 after exact-length body forwarding. Artifact: `benchmarks/artifacts/v1-model-group-lb-current-summary.json`. The fair same-mock 60-second 1M gate passed with RR delta `-0.005 ms` and weighted delta `+0.004 ms` versus a one-endpoint Model Group baseline. Artifact: `benchmarks/artifacts/v1-model-group-lb-1m-60s-gate-summary.json`.
- The headline 1M HTTP result at concurrency 200 is `+2.167 ms p50` and `+1.664 ms p99` router overhead with `0` non-200 responses.
- The headline 1M HTTPS result at concurrency 200 is `+6.539 ms p50` and `+35.845 ms p99` router overhead with `0` non-200 responses.
- Hard release thresholds still apply to the calibrated `1k`, `50k`, and `200k` gates. The `500k` and `1m` artifacts are published measurement proof and will become hard gates only after we have more repeated public baselines.
- “Fastest in the world” should be claimed only after public same-machine comparisons against named routers.

Model Group load-balancing smoke summary:

| Payload | Concurrency | Round-robin p50 overhead | Weighted p50 overhead | Result |
|---|---:|---:|---:|---|
| `1k` | 200 | `+0.329 ms` | `+0.364 ms` | ~`2,000 RPS`, `0` non-200 |
| `500k` | 200 | `+4.876 ms` | `+4.446 ms` | ~`80 RPS`, `0` non-200 |
| `1m` | 200 | `+9.739 ms` | `+10.679 ms` | ~`40 RPS`, `0` non-200 |

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

For a local Docker rebuild after editing Rust or after baking Portal changes into the image:

```bash
./scripts/rebuild_docker_local.sh
```

Pass a tag if you want a custom local image name:

```bash
./scripts/rebuild_docker_local.sh my-brighto-router:dev
```

The script builds the release binary, builds the Docker image, updates `.env` to use that local image with `BRIGHTO_ROUTER_PULL_POLICY=never`, restarts the router, and prints `./start.sh status`.

For an official multi-architecture Docker release, publish a manifest that contains both common server platforms:

```bash
PUSH_LATEST=1 ./scripts/docker_multiarch_release.sh v1.1.0
docker buildx imagetools inspect thusinh1969/brighto_airouter:v1.1.0
docker buildx imagetools inspect thusinh1969/brighto_airouter:latest
```

That release path publishes Linux containers for `linux/amd64` and `linux/arm64`. Windows and macOS are supported through Docker Desktop or WSL2; native `.exe` and `.app` binaries are a separate release channel and are not needed for the one-line Docker install.

Maintainers need Docker registry login, the Rust toolchain, Python with `requests`, Node.js, OpenSSL, and Chromium for release verification. Non-native runtime tests require QEMU; Docker Desktop includes it. The script builds both images locally and runs the HTTP/HTTPS SDK and Playwright checks on each before publishing a shared tag. Add `--check` after the tag to build and test without publishing. ARM validation under emulation is a functional check, not a native hardware performance benchmark.

Developer and release-support scripts are cataloged in [scripts/README.md](scripts/README.md). User-facing feature smoke tests live under `smoke/<feature>/` so the root install path stays simple.

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
