# Install BrighTO-Router

This file is the operational install guide. The short version is in [README.md](README.md).

## Local first run

One-line install:

```bash
curl -fsSL https://raw.githubusercontent.com/thusinh1969/BrighTO_Router/main/install.sh | bash
```

This clones or updates the repo in `$HOME/brighto-router` and runs `./start.sh install`. To choose another directory:

```bash
curl -fsSL https://raw.githubusercontent.com/thusinh1969/BrighTO_Router/main/install.sh | BRIGHTO_INSTALL_DIR=/opt/brighto-router bash
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

A failed Model Group backend is retried after `BACKEND_CIRCUIT_OPEN_SECONDS` seconds. The default is `30`; change it in `.env`, then run `./start.sh restart`. `ROUTER_WORKER_THREADS` is blank by default, which means the router uses all available CPU threads; set it only when you want to cap CPU use.

BrighTO-Router 1.1.0 uses `thusinh1969/brighto_airouter:v1.1.0` by default. The published tag includes `linux/amd64` and `linux/arm64`; Docker selects the image matching your host. Existing local `.env` files from older installs should include:

```bash
BRIGHTO_ROUTER_IMAGE=thusinh1969/brighto_airouter:v1.1.0
BACKEND_CIRCUIT_OPEN_SECONDS=30
```

### Supported install environments

The installer needs Bash, Python 3.9+, Git, and `curl`, in addition to Docker
with its Compose plugin. Python is only for installation/client scripts;
the Rust router does not need a Python runtime in its image.

Docker Desktop users need version 4.34 or later with **Settings → Resources →
Network → Enable host networking** selected and Linux containers enabled.
Run Windows install commands inside a WSL2 Linux terminal. The supplied Compose
configuration uses host networking so the router can reach localhost PostgreSQL
and local model servers. See [Docker's host-networking guide](https://docs.docker.com/engine/network/drivers/host/#docker-desktop).

| Environment | How it runs |
|---|---|
| Ubuntu/Debian on x86_64 servers | Docker pulls the `linux/amd64` image. |
| Ubuntu/Debian on ARM64 servers such as GB10 | Docker pulls the `linux/arm64` image. |
| macOS Intel | Docker Desktop runs the Linux `amd64` container. |
| macOS Apple Silicon | Docker Desktop runs the Linux `arm64` container. |
| Windows x86_64 | Docker Desktop with WSL2 runs the Linux `amd64` container. |

Native Windows `.exe` and macOS `.app` releases are not part of 1.1.0. Docker Compose is the install path. Both image architectures pass HTTP/HTTPS API and browser checks; ARM64 runtime verification uses QEMU emulation, not a native GB10 or Apple Silicon benchmark. For custom builds, use [the local Docker build instructions](README.md#portal-front-end-development).

What happens:

1. `.env` is created from `.env.example` if it does not exist.
2. Docker starts PostgreSQL 16.
3. SQL migrations run.
4. The default team and local demo client key are seeded.
5. Docker pulls and starts `thusinh1969/brighto_airouter:v1.1.0`.

Open on the same server:

```text
http://127.0.0.1:18080/
```

Open from another machine by replacing `<SERVER_IP>` with the server address:

```text
http://<SERVER_IP>:18080/
```

Admin login uses the `ADMIN_MASTER_KEY` generated in `.env` during install. Keep `.env` private.

Fresh install defaults `ADMIN_ALLOW_CIDR=0.0.0.0/0,::/0` so the generated random admin key works from the Portal URL printed by the installer, including a browser on another machine. The admin key is still required. For production, restrict admin source IPs by setting `ADMIN_ALLOW_CIDR` in `.env` to your office, VPN, bastion host, or reverse-proxy range, then restart:

```bash
# edit .env and set one allowed public IP, for example:
ADMIN_ALLOW_CIDR=<YOUR_PUBLIC_IP>/32

./start.sh restart
```

If the Portal says `Admin access blocked: ip not allowed`, your browser IP is outside `ADMIN_ALLOW_CIDR`; widen the range or temporarily use `0.0.0.0/0,::/0`, then restart.

To install directly with HTTPS and a local self-signed certificate:

```bash
./start.sh install --https --host <SERVER_HOST_OR_IP>
```

## Daily commands

| Command | Meaning |
|---|---|
| `./start.sh install` | First-time local HTTP install. Safe to rerun. |
| `./start.sh install --https --host HOST` | First-time HTTPS install with a self-signed certificate. |
| `./start.sh start` | Start Postgres when local, run migrations/seed, start router. Keeps existing PostgreSQL data. |
| `./start.sh stop` | Stop the Docker Compose stack. |
| `./start.sh restart` | Run migrations/seed and recreate router. Keeps existing PostgreSQL data. |
| `./start.sh upgrade` | Backup, pull the configured router image, run migrations/seed, and recreate only the router. Keeps existing PostgreSQL data. |
| `./start.sh backup` | Write `backups/brighto-backup-*/db.dump` plus `.env.backup`. |
| `./start.sh restore DIR --yes` | Restore a backup into the current database. Destructive; use `--with-env` only when moving credentials too. |
| `./start.sh status` | Show containers plus `/healthz` and `/readyz`. |
| `./start.sh logs` | Follow router logs. |
| `./start.sh migrate` | Run SQL migrations only. |
| `./start.sh seed` | Seed default team/demo key and missing provider templates only. It does not overwrite edited providers or model routes. |
| `./start.sh set-key openai sk-...` | Store a cloud provider key in `.env` and recreate router if running. The Add model route wizard can also accept a pasted route key. |
| `./start.sh smoke` | Run a short non-release benchmark smoke. |

Data safety: `docker build`, `docker compose up -d --force-recreate router`, `./start.sh start`, `./start.sh stop`, `./start.sh restart`, and `./start.sh upgrade` keep the local PostgreSQL volume. Do not run `docker compose down -v`, `docker volume rm brighto-airouter_pg-data`, or manual reset/truncate SQL unless you want to erase local routes, teams, keys, and usage.

For normal upgrades, do not export/import manually:

```bash
cd ~/brighto-router
./start.sh upgrade
```

The upgrade command creates a private backup in `backups/`, pulls the configured Docker image, runs migrations, seeds missing defaults only, and recreates the router container. Existing teams, provider endpoints, model routes, Model Groups, API keys, usage ledger, and `.env` provider keys stay in place.

If `.env` still has the old LAN-only admin default `127.0.0.1/32,::1/128,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16`, upgrade changes it to `0.0.0.0/0,::/0` so the Portal URL printed by `./start.sh status` can log in immediately. Custom `ADMIN_ALLOW_CIDR` values are preserved.

For server moves or manual rollback:

```bash
./start.sh backup
./start.sh restore backups/brighto-backup-YYYYMMDD-HHMMSS --yes
```

`restore` replaces the current database. Use `--with-env` only when you intentionally want to restore saved credentials from `.env.backup`.

Developer note: the default Compose policy pulls the official Docker image. If you are testing a locally built image with the same tag, set `BRIGHTO_ROUTER_PULL_POLICY=never` in `.env`, then run `docker compose up -d --force-recreate router`.

For the common local developer flow after editing Rust or after baking Portal HTML into the image, use:

```bash
./scripts/rebuild_docker_local.sh
```

For live Portal HTML/CSS/JS editing without rebuilding, set this once in `.env`, restart, then edit `static/index.html` and refresh the browser:

```bash
PORTAL_STATIC_FILE=/app/static/index.html
./start.sh restart
```

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
openai anthropic gemini deepseek kimi qwen dashscope zai openrouter jina voyage cohere meta-muse custom-llm ollaya
```

For providers that do not expose a compatible `/models` endpoint, type the provider model name manually and still use **Test connection** before saving enabled.

## Add a Model Group

API model names are unique across normal routes and Model Groups because client apps use that single string in requests.

A Model Group exposes one API model name backed by two or more existing tested routes of the same API shape. Use it when you want load balancing or failover behind one stable model name.

Use the portal:

1. Enter the admin key.
2. Open **Models & Routes**.
3. Click **Create Model Group**.
4. Choose the model type.
5. Set the API model name your apps will call.
6. Choose **Round robin** for equal traffic or **Weighted round robin** for uneven capacity.
7. Add two or more existing tested routes from the compatible-route dropdown.
8. Save enabled. Provider URLs and provider API keys are not entered in the group wizard; they stay on the source routes.

1.1.0 Model Groups require all selected routes to match the same API shape. Do not mix Chat Completions (`/v1/chat/completions`), Completions (`/v1/completions`), Responses (`/v1/responses`), embeddings, rerank, ASR, System One, or Anthropic Messages inside one group.

## Test from the command line

After saving a model route and creating a client API key, use `test_router.py`, which calls the single-file `brighto.py` SDK. Keep both files together. Python 3.9+ and `requests` are required for these clients; the Docker router does not need Python. Full SDK examples for every API shape are in [docs/API_EXAMPLES.md](docs/API_EXAMPLES.md).

```bash
python3 -m pip install requests
python3 test_router.py --router http://127.0.0.1:18080 --api-key sk-brighto-... --model <public-model-name> --text "Reply OK"
```

If you use the generated local demo key, the script can read it from `.env`:

```bash
python3 test_router.py --mode chat --model <public-chat-route> --text "Reply OK"
```

Other quick modes:

```bash
python3 test_router.py --mode completions --model <public-completions-route> --text "hello"
python3 test_router.py --mode responses --model <public-responses-route> --text "hello"
python3 test_router.py --mode embeddings --model <public-embedding-route> --text "hello"
python3 test_router.py --mode rerank --model <public-rerank-route> --query "router speed" --document "fast Rust gateway" --document "slow proxy" --top-n 1
python3 test_router.py --mode asr --model <public-asr-route> --file tests/fixtures/asr_smoke.wav
python3 test_router.py --mode systemone --model <public-systemone-route> --text "Choose refund or replacement."
python3 test_router.py --mode messages --model <public-anthropic-route> --text "Reply OK"
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

1.1.0 live provider smoke tests:

```bash
python3 scripts/adapter_smoke.py --provider qwen --task embedding
python3 scripts/adapter_smoke.py --provider qwen --task rerank
python3 scripts/adapter_smoke.py --provider all --task all
python3 scripts/adapter_router_smoke.py
./smoke/systemone/run_ollaya_laya.sh
```

For self-signed HTTPS, add `--insecure`. Image and audio examples require a backend model that accepts OpenAI-style multimodal chat JSON.

## HTTPS with custom PEM files

For direct HTTPS from the router binary, place PEM files in `ssl/`, mount `./ssl:/certs:ro` into the router container, and set.

If you do not have a real certificate yet, create a local self-signed certificate first:

```bash
./start.sh make-self-signed-cert llm-host.local
./start.sh tls --cert ssl/fullchain.pem --key ssl/privkey.pem --host llm-host.local --port 18443
```

Replace `llm-host.local` with the hostname clients use. Trust the generated public `ssl/ca.pem` in clients; never distribute `ssl/privkey.pem`. The helper signs a server certificate with a local CA and removes the temporary CA private key.

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
