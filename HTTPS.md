# HTTPS setup

BrighTO-Router can serve HTTPS directly or run behind an existing HTTPS reverse proxy. Direct HTTPS needs a certificate chain and its matching private key in `ssl/`. This folder stays outside Git.

## First install

For a private server without a public certificate, replace `llm-host.local` with the hostname your clients will open:

```bash
./start.sh install --https --host llm-host.local
./start.sh status
```

The installer generates a local certificate authority (CA), signs a server certificate for that hostname, and enables HTTPS. It prints the Portal URL and creates a random admin key in `.env`. Log in with username `admin` and that key. Restrict `ADMIN_ALLOW_CIDR` to your office or VPN network; [installation and admin access](INSTALL.md) explain the settings.

## Enable HTTPS on an existing install

```bash
./start.sh make-self-signed-cert llm-host.local
./start.sh tls --cert ssl/fullchain.pem --key ssl/privkey.pem --host llm-host.local --port 18443
./start.sh status
```

The helper creates these files:

| File | Purpose |
|---|---|
| `ssl/fullchain.pem` | Server certificate followed by the local CA certificate; mounted into Docker. |
| `ssl/privkey.pem` | Server private key; keep it on the server. |
| `ssl/ca.pem` | Public CA certificate; distribute it to clients that must trust this server. |

The temporary CA private key is removed after signing. Running the helper again creates a new CA; clients must trust the new `ca.pem`. A hostname or IP used by clients must appear in the server certificate. Use the same hostname in the helper, Portal URL, and client configuration.

The `tls` command copies PEM files into `ssl/`, updates `.env`, and recreates the router. Compose already mounts this folder read-only. Container paths are `/certs/fullchain.pem` and `/certs/privkey.pem`.

## Use a publicly trusted certificate

Use the full chain and matching key supplied by your certificate issuer:

```bash
./start.sh tls --cert /path/to/fullchain.pem --key /path/to/privkey.pem --host router.example.com --port 18443
```

Clients normally trust the issuer without an extra CA setting. Certificate renewal is managed by your issuer or existing certificate tooling; restart the router after replacing its PEM files. If a reverse proxy already handles HTTPS, leave `TLS_CERT_PATH` and `TLS_KEY_PATH` empty and run the router on HTTP behind it.

## Trust a local CA

Browsers show a warning until the local CA is trusted. Import `ssl/ca.pem` into the client device's trusted certificate store. Give clients only this public certificate, never `privkey.pem`.

Verify both certificate trust and the hostname:

```bash
curl --cacert ssl/ca.pem https://llm-host.local:18443/healthz
curl --cacert ssl/ca.pem https://llm-host.local:18443/readyz
```

Python and agent clients can use a CA bundle directly. Preserve public certificate trust if the same client also downloads tools or accesses public HTTPS services:

```bash
python3 - <<'PYTHON'
from pathlib import Path
import ssl

roots = ssl.create_default_context().get_ca_certs(binary_form=True)
pem = "".join(ssl.DER_cert_to_PEM_cert(root) for root in roots)
Path("ssl/client-ca-bundle.pem").write_text(pem + "\n" + Path("ssl/ca.pem").read_text())
PYTHON

export SSL_CERT_FILE=/absolute/path/to/client-ca-bundle.pem
export REQUESTS_CA_BUNDLE=/absolute/path/to/client-ca-bundle.pem
export NODE_EXTRA_CA_CERTS=/absolute/path/to/client-ca-bundle.pem
export CODEX_CA_CERTIFICATE=/absolute/path/to/client-ca-bundle.pem
```

`NODE_EXTRA_CA_CERTS` applies to Node-based clients such as Claude Code and OpenClaw. `CODEX_CA_CERTIFICATE` applies to Codex. See [agent client configuration](docs/AGENT_CLIENTS.md) for protocol and model settings. Do not disable certificate verification in production.

## Firewall and admin access

If Ubuntu's firewall is enabled:

```bash
sudo ufw allow 18443/tcp
```

Network reachability and admin permission are separate. A reachable Portal can still reject an admin request outside `ADMIN_ALLOW_CIDR`. Add the client's office/VPN range to that setting and restart; do not change the admin key to solve an IP restriction. The active protocol and listener port are reported by `./start.sh status`. The direct listener serves either HTTP or HTTPS; it does not serve both protocols on the same port.

## Switch back to HTTP

Clear `TLS_CERT_PATH` and `TLS_KEY_PATH` in `.env`, set `LISTEN_ADDR=0.0.0.0:18080` and `BASE_URL=http://llm-host.local:18080`, then run:

```bash
./start.sh restart
```

## Troubleshooting

| Symptom | Check |
|---|---|
| Certificate rejected by an agent | Trust `ssl/ca.pem`, check hostname coverage, and use a server certificate signed by that CA. |
| Admin says `ip not allowed` | Check `ADMIN_ALLOW_CIDR` against the client's actual IP or trusted proxy setup. |
| Router cannot read a PEM file | Run `./start.sh logs`; check filenames and Docker read permissions. The helper sets the permissions expected by the non-root image. |
| HTTP fails on the HTTPS port | Use `https://` for this listener; switch the configuration back to HTTP if needed. |
| Docker cannot see the certificate | Keep `./ssl:/certs:ro` mounted and use `/certs/...` paths in `.env`. |
