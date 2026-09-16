# CODEX AUDIT — Custom PEM HTTPS is prepared on this machine; implement TLS options

Date: 2026-09-17  
Role: Codex auditor/mentor. DeepSeek owns product code changes.

## Verdict

Custom PEM files are now prepared locally. DeepSeek must implement the HTTPS runtime/config path so these files can be used by Docker Compose and direct binary runs.

Do not add nginx/Caddy by default. Keep one Rust binary and one optional TLS mode.

## Files created locally

These files exist on this machine and are intentionally ignored by Git:

```text
ssl/fullchain.pem
ssl/privkey.pem
ssl/README.local.md
```

Certificate details:

- CN: `118.69.81.92`
- SAN:
  - `IP:118.69.81.92`
  - `IP:127.0.0.1`
  - `DNS:localhost`
  - `DNS:brighto-router`
- Valid: 2026-09-16 to 2036-09-13 UTC
- Private key mode: `600`

## Exact `.env` settings for this machine

Use these after TLS runtime support lands:

```bash
LISTEN_ADDR=0.0.0.0:18443
BASE_URL=https://118.69.81.92:18443
TLS_CERT_PATH=/certs/fullchain.pem
TLS_KEY_PATH=/certs/privkey.pem
```

Do not enable these yet unless the binary actually serves TLS. If current binary is still HTTP-only, setting port `18443` only moves HTTP to that port and confuses testing.

## Required Docker Compose mount

Router service must mount local `ssl/` into the container read-only:

```yaml
services:
  router:
    volumes:
      - router-data:/var/lib/brighto-router
      - ./ssl:/certs:ro
```

Container env paths must be `/certs/fullchain.pem` and `/certs/privkey.pem`, not host paths.

## Required Rust runtime behavior

At boot:

1. Read `TLS_CERT_PATH` and `TLS_KEY_PATH`.
2. If both are empty/missing: serve HTTP exactly like today.
3. If both are set: serve HTTPS on `LISTEN_ADDR` using those PEM files.
4. If only one is set: fail fast with clear error.
5. If PEM parse fails: fail fast with clear error.
6. Log protocol explicitly:
   - `protocol=http addr=...`
   - `protocol=https addr=...`

Implementation must not introduce OpenSSL/native-tls. Use Rustls. Current dependency note already says `axum-server` can enable `tls-rustls`; that is the preferred small change if compatible.

## Required `start.sh` behavior

Add flags without making normal install harder:

```bash
./start.sh install --https --tls-cert ./ssl/fullchain.pem --tls-key ./ssl/privkey.pem --https-port 18443
```

Behavior:

1. Validate cert/key exist.
2. Copy into `ssl/` if source path differs.
3. `chmod 600 ssl/privkey.pem` best effort.
4. Set `.env`:
   - `LISTEN_ADDR=0.0.0.0:<port>`
   - `BASE_URL=https://<host-or-ip>:<port>`
   - `TLS_CERT_PATH=/certs/fullchain.pem`
   - `TLS_KEY_PATH=/certs/privkey.pem`
5. Ensure docker-compose has `./ssl:/certs:ro` mount.
6. Restart/recreate router.

Also add simpler helper:

```bash
./start.sh tls --cert ./ssl/fullchain.pem --key ./ssl/privkey.pem --host 118.69.81.92 --port 18443
./start.sh restart
```

Do not require Kubernetes users to use this. K8s should mount cert/key from Secret with the same env names.

## Required healthcheck behavior

Current `healthcheck` builds `http://{LISTEN_ADDR}/healthz`. That will fail under HTTPS.

Fix:

- if TLS envs are set, healthcheck uses `https://`;
- for self-signed local cert, use reqwest option equivalent to accepting invalid cert only for `healthcheck`, or provide Compose healthcheck using `curl -k` if curl exists;
- do not disable certificate validation for upstream provider calls.

Simpler acceptable Compose healthcheck:

```yaml
healthcheck:
  test: ["CMD-SHELL", "if [ -n \"$${TLS_CERT_PATH:-}\" ]; then /app/brighto-router healthcheck --insecure-local-tls; else /app/brighto-router healthcheck; fi"]
```

## Required tests on this machine

After implementing TLS:

```bash
./start.sh restart
curl -k https://127.0.0.1:18443/healthz
curl -k https://118.69.81.92:18443/readyz
curl -k https://118.69.81.92:18443/
```

Expected:

- all HTTPS checks succeed;
- HTTP to that port does not pretend to work:

```bash
curl -v http://127.0.0.1:18443/healthz
```

Expected: TLS/connection error, not a valid HTTP response.

## Required Playwright check

Use HTTPS portal URL:

```text
https://118.69.81.92:18443/
```

For self-signed cert, Playwright must launch with `ignoreHTTPSErrors: true` or equivalent.

Check:

1. Login Admin.
2. F5 refresh stays logged in after session restore.
3. Dashboard loads.
4. Create/edit/delete route flow works.
5. Local llama no-auth route flow works.

## Security rule

Do not commit anything under `ssl/`. `.gitignore` already includes:

```text
ssl/
*.pem
*.key
```

Provider keys and TLS private keys are different secrets. Neither should appear in command-line args, git commits, terminal logs, or audit files.
