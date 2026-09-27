# System One Ollaya/Laya smoke

This smoke test proves BrighTO-Router can route TypeSafe/Jev-compatible System One decision traffic through a real local Ollaya server running Laya.

What it does:

1. Starts `ghcr.io/ollaya-dev/ollaya:latest` in Docker.
2. Pulls the `laya` decision model.
3. Sends one direct `/v1/systemone` request to Ollaya.
4. Creates two BrighTO System One routes and one round-robin Model Group.
5. Calls BrighTO `/v1/systemone` and `/v1/decisions`.
6. Waits past the health interval and calls again, catching auth/health regressions.

No-auth local mode:

```bash
./smoke/systemone/run_ollaya_laya.sh
```

Auth-required mode:

```bash
OLLAYA_API_KEY="test-systemone-key" ./smoke/systemone/run_ollaya_laya.sh
```

Useful overrides:

```bash
ROUTER_URL=https://my-router:18443 \
OLLAYA_PORT=11437 \
OLLAYA_MODEL=laya \
BRIGHTO_PUBLIC_MODEL=ollaya-laya \
BRIGHTO_GROUP_MODEL=ollaya-laya-group \
./smoke/systemone/run_ollaya_laya.sh
```

The script uses the BrighTO admin and demo client keys from `.env`. It does not print provider keys.
