# Install BrighTO-Router

This file is the operational install guide. The short version is in [README.md](README.md).

## Local first run

```bash
git clone https://github.com/thusinh1969/Brighto_AIRouter.git
cd Brighto_AIRouter
./start.sh install
./start.sh status
```

What happens:

1. `.env` is created from `.env.example` if it does not exist.
2. Docker starts PostgreSQL 16.
3. SQL migrations run.
4. Default team and provider templates are seeded.
5. Docker pulls and starts `thusinh1969/brighto_airouter:v1`.

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

If the page opens but **Load providers** returns `403: ip not allowed`, allow your client network in `.env` and restart:

```bash
ADMIN_ALLOW_CIDR=0.0.0.0/0,::/0
./start.sh restart
```

For shared or production use, replace `ADMIN_MASTER_KEY` and narrow `ADMIN_ALLOW_CIDR` to your VPN, office subnet, or reverse proxy.

## Daily commands

| Command | Meaning |
|---|---|
| `./start.sh install` | First-time local install. Safe to rerun. |
| `./start.sh start` | Start Postgres when local, run migrations/seed, start router. |
| `./start.sh stop` | Stop the Docker Compose stack. |
| `./start.sh restart` | Run migrations/seed and recreate router. |
| `./start.sh status` | Show containers plus `/healthz` and `/readyz`. |
| `./start.sh logs` | Follow router logs. |
| `./start.sh migrate` | Run SQL migrations only. |
| `./start.sh seed` | Seed default team and provider templates only. |
| `./start.sh set-key openai sk-...` | Store a provider key in `.env` and recreate router if running. |
| `./start.sh smoke` | Run a short non-release benchmark smoke. |

## Configure a provider

Example for OpenAI:

```bash
./start.sh set-key openai sk-your-key
./start.sh restart
```

Then use the portal:

1. Enter admin key.
2. Click **Load providers**.
3. Enable the provider.
4. Click **Fetch models**.
5. Choose a model and create a route.
6. Create a team API key for your application.

Provider key names supported by `set-key`:

```text
openai anthropic gemini deepseek kimi qwen zai openrouter meta-muse custom-openai
```

For providers that do not expose an OpenAI-style `/models` endpoint, type the model name manually in the portal.


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
chmod 600 ssl/privkey.pem
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

Local starter with an in-cluster development PostgreSQL:

```bash
./start.sh install --k8s --replicas 2
```

Production-style Kubernetes with an existing PostgreSQL database:

```bash
./start.sh install --database-url 'postgres://user:pass@db-host:5432/brighto_router' --k8s --replicas 2
```

The Kubernetes command creates or updates the namespace, secret, config map, deployment, and service. With local K8s PostgreSQL, it waits for PostgreSQL and runs migrations/seed inside the cluster before starting the router. With external PostgreSQL, it runs migrations/seed against the supplied database URL and skips the development PostgreSQL manifest.

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
