# System One backend examples

Use [Quyết](https://github.com/ncchinh/quyet) for an independent Python decision
backend, or Ollaya/Laya for the Docker smoke below. Both use the same BrighTO
System One route type and Python SDK.

## Quyết-1.0-Small

The [Quyết setup and Python client example](../../docs/API_EXAMPLES.md#quyet-system-one-backend)
includes environment setup, model download, optional backend authentication,
Portal configuration, and inference calls. [quyet_server.py](quyet_server.py)
adds an optional HTTP interface around upstream `predict(state, questions)`;
inference remains outside the Rust router.

For multiple GPUs, run one Small replica per GPU, create one tested route for
each replica, then create a System One Model Group. The client calls the group
name. Small is not a chat model and its context limit is independent of the
Router's large-context chat benchmarks.

## Ollaya/Laya smoke

This smoke test proves BrighTO-Router can route TypeSafe/Jev-compatible System One decision traffic through a real local Ollaya server running Laya.

It is meant to be copy/paste friendly for a fresh BrighTO install. It does not require a paid provider key.

## Prerequisites

Run these from the BrighTO-Router repository root.

```bash
cd ~/brighto-router   # or your local BrighTO_Router checkout
./start.sh status
```

The router should be healthy. The smoke script reads `BASE_URL` from `.env`, so both default HTTP installs and HTTPS installs work without extra flags. A normal local HTTP install uses:

```text
http://127.0.0.1:18080
```

A HTTPS install usually uses a value like:

```text
https://<router-host>:18443
```

If you want to override `.env`, pass `ROUTER_URL` in the commands below.

## One-command local smoke, no provider API key

This starts Ollaya in Docker, pulls Laya, creates two BrighTO System One routes, creates one round-robin Model Group, calls the router, waits past the backend health interval, then calls the router again.

```bash
./smoke/systemone/run_ollaya_laya.sh
```

Expected success lines include:

```text
direct_ollaya_ok ['duplicate_charge']
brighto_test_connection_ok systemone answers 2
gateway_ok /v1/systemone ... 200 ['duplicate_charge', 'team']
gateway_ok /v1/decisions ... 200 ['duplicate_charge', 'team']
System One Ollaya/Laya smoke completed.
```

## HTTPS router example

Use this when you want to override `.env` and point the smoke at a specific HTTPS BrighTO Portal/API, for example `https://<router-host>:18443`:

```bash
ROUTER_URL=https://<router-host>:18443 \
  ./smoke/systemone/run_ollaya_laya.sh
```

The script accepts self-signed BrighTO certificates for this smoke test.

## Auth-required Ollaya/JEV-like backend

This proves provider Bearer auth is forwarded correctly. Ollaya will reject direct calls without the key, and BrighTO must call it with the saved provider key.

```bash
OLLAYA_API_KEY="test-systemone-key" \
  ./smoke/systemone/run_ollaya_laya.sh
```

With a remote HTTPS router:

```bash
ROUTER_URL=https://<router-host>:18443 \
OLLAYA_API_KEY="test-systemone-key" \
  ./smoke/systemone/run_ollaya_laya.sh
```

## Custom port, model, and public route names

Use this when port `11435` is busy, or when you want route names that will not collide with existing test data.

```bash
ROUTER_URL=http://127.0.0.1:18080 \
OLLAYA_PORT=11437 \
OLLAYA_CONTAINER=brighto-ollaya-laya-smoke-11437 \
OLLAYA_DATA_DIR=/tmp/brighto-ollaya-laya-smoke-11437 \
OLLAYA_MODEL=laya \
BRIGHTO_PUBLIC_MODEL=ollaya-laya-smoke \
BRIGHTO_GROUP_MODEL=ollaya-laya-smoke-group \
  ./smoke/systemone/run_ollaya_laya.sh
```

For HTTPS:

```bash
ROUTER_URL=https://<router-host>:18443 \
OLLAYA_PORT=11437 \
OLLAYA_CONTAINER=brighto-ollaya-laya-smoke-11437 \
OLLAYA_DATA_DIR=/tmp/brighto-ollaya-laya-smoke-11437 \
BRIGHTO_PUBLIC_MODEL=ollaya-laya-smoke \
BRIGHTO_GROUP_MODEL=ollaya-laya-smoke-group \
  ./smoke/systemone/run_ollaya_laya.sh
```

## What the script creates

The script uses the BrighTO admin key and demo client key from `.env`. It does not print provider keys.

It creates temporary data in BrighTO:

- two provider backends pointing at `http://127.0.0.1:<OLLAYA_PORT>/v1`;
- two System One model routes;
- one System One round-robin Model Group;
- one temporary BrighTO client API key allowed to call those routes.

It also starts one Docker container named by `OLLAYA_CONTAINER`, default `brighto-ollaya-laya-smoke`.

## Clean up the Ollaya smoke container

The BrighTO routes remain in PostgreSQL so you can inspect them in the Portal. To stop only the Ollaya smoke container:

```bash
docker rm -f brighto-ollaya-laya-smoke
```

If you used a custom container name:

```bash
docker rm -f brighto-ollaya-laya-smoke-11437
```

## Troubleshooting

If the script says `.env not found`, run:

```bash
./start.sh install
```

If Ollaya data directory is not writable, fix it once:

```bash
sudo chown -R "$(id -u):$(id -g)" "$HOME/.ollaya-brighto-smoke"
```

If the router is on another host, always pass `ROUTER_URL`:

```bash
ROUTER_URL=https://<router-host>:18443 \
  ./smoke/systemone/run_ollaya_laya.sh
```

If a model or group name already exists, pass unique names:

```bash
BRIGHTO_PUBLIC_MODEL=ollaya-laya-$(date +%s) \
BRIGHTO_GROUP_MODEL=ollaya-laya-group-$(date +%s) \
  ./smoke/systemone/run_ollaya_laya.sh
```
