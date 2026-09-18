# HTTPS with custom PEM files on Ubuntu

This guide shows the simple direct-HTTPS setup for a self-hosted BrighTO-Router server.

Default BrighTO-Router can run behind any existing HTTPS load balancer or reverse proxy. If you want the router itself to serve HTTPS, use custom PEM files and mount them into the Docker container.

> Runtime requirement: the router binary must support `TLS_CERT_PATH` and `TLS_KEY_PATH`. If your current image does not support these settings yet, use this file as the implementation contract for the next image, or terminate HTTPS in your existing proxy until the TLS build is released.

## What you will create

On the Ubuntu host:

```text
BrighTO_Router/
  .env
  docker-compose.yml
  ssl/
    fullchain.pem
    privkey.pem
```

`ssl/` is local-only and must never be committed to Git.

## 1. Install basics on Ubuntu

```bash
sudo apt update
sudo apt install -y git curl openssl docker.io docker-compose-plugin
sudo usermod -aG docker "$USER"
```

Log out and log in again if Docker was just installed.

## 2. Pull the repo

```bash
git clone https://github.com/thusinh1969/BrighTO_Router.git
cd BrighTO_Router
```

## 3. Create custom PEM files

For a private LAN or first test, create a self-signed certificate.

Replace `<SERVER_IP>` with the server IP that browsers will use.

```bash
mkdir -p ssl

openssl req -x509 -newkey rsa:4096 -sha256 -days 3650 -nodes \
  -keyout ssl/privkey.pem \
  -out ssl/fullchain.pem \
  -subj "/CN=<SERVER_IP>" \
  -addext "subjectAltName=IP:<SERVER_IP>,DNS:localhost,IP:127.0.0.1"

chmod 644 ssl/privkey.pem
chmod 644 ssl/fullchain.pem
```

For a real domain, copy your real certificate files instead:

```bash
mkdir -p ssl
cp /path/to/fullchain.pem ssl/fullchain.pem
cp /path/to/privkey.pem ssl/privkey.pem
chmod 644 ssl/privkey.pem
chmod 644 ssl/fullchain.pem
```

## 4. Configure `.env` for HTTPS

Create `.env` if it does not exist:

```bash
cp -n .env.example .env
```

Edit these values:

```bash
LISTEN_ADDR=0.0.0.0:18443
BASE_URL=https://<SERVER_IP>:18443
TLS_CERT_PATH=/certs/fullchain.pem
TLS_KEY_PATH=/certs/privkey.pem
ADMIN_MASTER_KEY=brightoIsGreat@2026
ADMIN_ALLOW_CIDR=0.0.0.0/0,::/0
```

For shared or production use, replace `ADMIN_MASTER_KEY` and narrow `ADMIN_ALLOW_CIDR` to your VPN, office subnet, or private network.

If you want standard HTTPS port 443:

```bash
LISTEN_ADDR=0.0.0.0:443
BASE_URL=https://your-domain.example
```

Port 443 may already be used by another service. Use `18443` for the simplest first test.

## 5. Mount `ssl/` into Docker

In `docker-compose.yml`, the router service must mount `ssl/` read-only:

```yaml
services:
  router:
    volumes:
      - router-data:/var/lib/brighto-router
      - ./ssl:/certs:ro
```

The cert paths in `.env` are container paths, so they must be `/certs/fullchain.pem` and `/certs/privkey.pem`, not host paths.

## 6. Start or restart

```bash
./start.sh install
./start.sh restart
./start.sh status
```

If you already installed before, this is enough:

```bash
./start.sh restart
```

## 7. Test HTTPS

Self-signed certificate test:

```bash
curl -k https://127.0.0.1:18443/healthz
curl -k https://<SERVER_IP>:18443/readyz
```

Real trusted certificate test:

```bash
curl https://your-domain.example/healthz
curl https://your-domain.example/readyz
```

Open the portal:

```text
https://<SERVER_IP>:18443/
```

With a self-signed certificate, the browser will show a warning. That is expected. For production, use a certificate trusted by browsers.

## 8. Firewall

If Ubuntu firewall is enabled:

```bash
sudo ufw allow 18443/tcp
sudo ufw status
```

For port 443:

```bash
sudo ufw allow 443/tcp
```

## 9. How to switch back to HTTP

Remove or blank these two lines from `.env`:

```bash
TLS_CERT_PATH=
TLS_KEY_PATH=
```

Set the URL back to HTTP:

```bash
LISTEN_ADDR=0.0.0.0:18080
BASE_URL=http://<SERVER_IP>:18080
```

Restart:

```bash
./start.sh restart
```

## 10. Troubleshooting

| Symptom | Check |
|---|---|
| Router does not start | `./start.sh logs` and verify both PEM files exist inside `ssl/`. |
| `curl http://...` fails | HTTPS listener expects `https://`, not `http://`. |
| Browser warning | Self-signed certificates are not trusted by default. Use a real certificate for production. |
| `permission denied` reading key | Check `ssl/privkey.pem` owner and permissions. |
| Docker cannot see cert | Confirm `./ssl:/certs:ro` is mounted in `docker-compose.yml`. |
| Healthcheck still uses HTTP | The image/start script must support HTTPS healthcheck when TLS env vars are set. |
