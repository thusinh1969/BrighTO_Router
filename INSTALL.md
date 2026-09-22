# Install BrighTO-Router

This file is the operational install guide. The short version is in [README.md](README.md).

## Local first run

One-line install:

```bash
curl -fsSL https://raw.githubusercontent.com/thusinh1969/BrighTO_Router/main/install.sh | bash
```

This clones or updates the repo in `$HOME/brighto-router` and runs `./start.sh install`. To choose another directory:

```bash
BRIGHTO_INSTALL_DIR=/opt/brighto-router curl -fsSL https://raw.githubusercontent.com/thusinh1969/BrighTO_Router/main/install.sh | bash
```

If you want to inspect the installer before running it:

```bash
curl -fsSLO https://raw.githubusercontent.com/thusinh1969/BrighTO_Router/main/install.sh
less install.sh
bash install.sh
```

Manual install:

```bash
git clone https://github.com/thusinh1969/BrighTO_Router.git
cd BrighTO_Router
./start.sh install
./start.sh status
```

This preview-3 line uses `thusinh1969/brighto_airouter:preview-3` by default and is intended to become `main` after final field feedback. Existing local `.env` files from older installs should include:

```bash
BRIGHTO_ROUTER_IMAGE=thusinh1969/brighto_airouter:preview-3
```

What happens:

1. `.env` is created from `.env.example` if it does not exist.
2. Docker starts PostgreSQL 16.
3. SQL migrations run.
4. The default team and local demo client key are seeded.
5. Docker pulls and starts `thusinh1969/brighto_airouter:preview-3`.

Open on the same server:

```text
http://127.0.0.1:18080/
```

Open from another machine by replacing `<SERVER_IP>` with the server address:

```text
http://<SERVER_IP>:18080/
```

Default local admin key:

```text
brightoIsGreat@2026
```

Fresh install allows Admin Portal access from any IP so first-time remote testing works immediately.

For shared or production use, replace `ADMIN_MASTER_KEY` and narrow `ADMIN_ALLOW_CIDR` in `.env` to your VPN, office subnet, or reverse proxy, then restart:

```bash
./start.sh restart
```

## Daily commands

| Command | Meaning |
|---|---|
| `./start.sh install` | First-time local install. Safe to rerun. |
| `./start.sh start` | Start Postgres when local, run migrations/seed, start router. Keeps existing PostgreSQL data. |
| `./start.sh stop` | Stop the Docker Compose stack. |
| `./start.sh restart` | Run migrations/seed and recreate router. Keeps existing PostgreSQL data. |
| `./start.sh status` | Show containers plus `/healthz` and `/readyz`. |
| `./start.sh logs` | Follow router logs. |
| `./start.sh migrate` | Run SQL migrations only. |
| `./start.sh seed` | Seed default team/demo key and missing provider templates only. It does not overwrite edited providers or model routes. |
| `./start.sh set-key openai sk-...` | Store a cloud provider key in `.env` and recreate router if running. The Add model route wizard can also accept a pasted route key. |
| `./start.sh smoke` | Run a short non-release benchmark smoke. |

Data safety: `docker build`, `docker compose up -d --force-recreate router`, `./start.sh start`, `./start.sh stop`, and `./start.sh restart` keep the local PostgreSQL volume. Do not run `docker compose down -v`, `docker volume rm brighto-airouter_pg-data`, or manual reset/truncate SQL unless you want to erase local routes, teams, keys, and usage.

## Add a model route

The first useful setup is a model route. A route exposes one API model name to your applications and points it to one upstream provider model.

Use the portal:

1. Enter the admin key.
2. Open **Models & Routes**.
3. Click **Add model route**.
4. Pick a provider preset or **Custom LLM**.
5. Enter the Base URL and provider API key when required. You can paste the key in the wizard, leave it blank to use the matching `.env` key when configured, or leave it blank for local/no-auth **Custom LLM** endpoints.
6. Click **Load models**, choose one model, then click **Test connection**.
7. Click **Save enabled** only after the test passes.
8. Create or reuse a client API key under **API Keys**.

Provider key names supported by `set-key` if you prefer `.env` secrets:

```text
openai anthropic gemini deepseek kimi qwen dashscope zai openrouter jina voyage cohere meta-muse custom-llm
```

For providers that do not expose a compatible `/models` endpoint, type the provider model name manually and still use **Test connection** before saving enabled.

## Add a Model Group

API model names are unique across normal routes and Model Groups because client apps use that single string in requests.

A Model Group exposes one API model name backed by two or more existing tested routes of the same type. Use it when you want load balancing or failover behind one stable model name.

Use the portal:

1. Enter the admin key.
2. Open **Models & Routes**.
3. Click **Create Model Group**.
4. Choose the model type.
5. Set the API model name your apps will call.
6. Choose **Round robin** for equal traffic or **Weighted round robin** for uneven capacity.
7. Add two or more existing tested routes from the compatible-route dropdown.
8. Save enabled. Provider URLs and provider API keys are not entered in the group wizard; they stay on the source routes.

Preview-3 groups require all selected routes to match the same model type. Do not mix chat, embeddings, rerank, ASR, or Anthropic Messages inside one group.

## Test from the command line

After saving a model route and creating a client API key, run one request with the helper script:

```bash
python3 test_router.py --router http://127.0.0.1:18080 --api-key sk-brighto-... --model <public-model-name> --text "Reply OK"
```

If you use the seeded local demo key, the script can read it from `.env`:

```bash
python3 test_router.py --model <public-model-name> --text "Reply OK"
```

Other quick modes:

```bash
python3 test_router.py --mode embeddings --model <public-embedding-route> --text "hello"
python3 test_router.py --mode rerank --model <public-rerank-route> --query "router speed" --document "fast Rust gateway" --document "slow proxy" --top-n 1
python3 test_router.py --mode asr --model <public-asr-route> --file tests/fixtures/asr_smoke.wav
python3 test_router.py --provider qwen --mode embeddings --text "hello"
python3 test_router.py --provider qwen --mode rerank --query "router speed"
python3 test_router.py --provider jina --mode embeddings --text "hello"
python3 test_router.py --provider jina --mode rerank --query "router speed"
python3 test_router.py --provider voyage --mode embeddings --text "hello"
python3 test_router.py --provider voyage --mode rerank --query "router speed"
python3 test_router.py --provider cohere --mode rerank --query "router speed"
python3 test_router.py --model <vision-model-route> --text "Describe this image." --image ./photo.jpg
python3 test_router.py --model <audio-model-route> --text "Summarize this audio." --audio ./sample.wav
```

`--provider` uses standard public route names. Run `python3 test_router.py --list-presets` to see the mapping, or pass `--model` when your route name is custom. In rerank mode, `--query` and `--text` both work; `--query` is clearer and takes priority.

Preview-3 live provider smoke tests:

```bash
python3 scripts/adapter_smoke.py --provider qwen --task embedding
python3 scripts/adapter_smoke.py --provider qwen --task rerank
python3 scripts/adapter_smoke.py --provider all --task all
python3 scripts/adapter_router_smoke.py
```

For self-signed HTTPS, add `--insecure`. Image and audio examples require a backend model that accepts OpenAI-style multimodal chat JSON.

## HTTPS with custom PEM files

For direct HTTPS from the router binary, place PEM files in `ssl/`, mount `./ssl:/certs:ro` into the router container, and set.

If you do not have a real certificate yet, create a local self-signed certificate first:

```bash
SERVER_IP=$(hostname -I | awk '{print $1}')
mkdir -p ssl
openssl req -x509 -newkey rsa:4096 -sha256 -days 3650 -nodes \
  -keyout ssl/privkey.pem \
  -out ssl/fullchain.pem \
  -subj "/CN=${SERVER_IP}" \
  -addext "subjectAltName=IP:${SERVER_IP},IP:127.0.0.1,DNS:localhost,DNS:brighto-router"
chmod 644 ssl/privkey.pem
chmod 644 ssl/fullchain.pem
```

Then set:

```bash
LISTEN_ADDR=0.0.0.0:18443
BASE_URL=https://<SERVER_IP>:18443
TLS_CERT_PATH=/certs/fullchain.pem
TLS_KEY_PATH=/certs/privkey.pem
```

Full Ubuntu example: [HTTPS.md](HTTPS.md).

## Existing PostgreSQL

Use this when your team already has a managed or shared PostgreSQL database:

```bash
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router'
```

The installer runs migrations and seeds provider templates against that database. The local PostgreSQL container is skipped.

## Kubernetes

Kubernetes is optional. Use it when your team already has a cluster, wants multiple router pods, or needs rolling upgrades. For a normal team trial, Docker Compose is simpler.

Local single-node starter with an in-cluster development PostgreSQL:

```bash
./start.sh install --k8s --replicas 2
```

Production-style single-node or multi-node Kubernetes with an existing PostgreSQL database:

```bash
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router' --k8s --replicas 3
```

The Kubernetes command creates or updates the namespace, secret, config map, deployment, and service. With local K8s PostgreSQL, it waits for PostgreSQL and runs migrations/seed inside the cluster before starting the router. With external PostgreSQL, it runs migrations/seed against the supplied database URL and skips the development PostgreSQL manifest.

For real multi-node use, do not use `k8s/postgres.dev.yaml`; it is a temporary development database. Use managed PostgreSQL or your own highly available PostgreSQL, put the router Service behind your Ingress or load balancer, and choose replicas based on traffic. The open-source manifests stay small on purpose. Enterprise packaging can add Helm-style configuration, autoscaling, pod disruption budgets, network policy, and production ingress templates without changing the router core.

## Portal UI development

The Portal front-end is one file: `static/index.html`. It contains HTML, CSS, and JavaScript. Rust embeds that file into the production binary.

For fast UI design work under Docker Compose, set this in `.env`:

```bash
PORTAL_STATIC_FILE=/app/static/index.html
docker compose up -d --force-recreate router
```

After that one restart, edit `static/index.html` and refresh the browser. Rebuild Docker only when Rust code changes or when you want the final Portal baked into the production image.

## Files operators usually edit

| File | Purpose |
|---|---|
| `.env` | Local Docker runtime secrets and settings. |
| `.env.example` | Template for first-time installs. |
| `docker-compose.yml` | Local/server Compose runtime. |
| `k8s/*.yaml` | Minimal Kubernetes starter manifests. |
| `scripts/seed_defaults.sql` | Default team and provider template seed. Safe to rerun. |

## Health checks

```bash
curl -fsS http://127.0.0.1:18080/healthz
curl -fsS http://127.0.0.1:18080/readyz
```

`healthz` means the process is alive. `readyz` means the router is ready to serve with loaded configuration.
