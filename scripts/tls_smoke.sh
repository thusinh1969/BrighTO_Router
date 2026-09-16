#!/usr/bin/env bash
# TLS smoke: generate a throwaway self-signed cert, boot the router in HTTPS mode, and verify
#  - https /healthz -> ok
#  - plain HTTP on the TLS port does NOT return a valid HTTP response
#  - startup log prints protocol=https
#  - the "healthcheck" subcommand succeeds under TLS (accepts the local self-signed cert)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
BIN="$ROOT/target/release/brighto-router"
[ -x "$BIN" ] || { echo "FAIL: build release first"; exit 1; }

TMP="$(mktemp -d)"
cleanup(){ kill "${RPID:-0}" 2>/dev/null || true; rm -rf "$TMP"; }
trap cleanup EXIT

openssl req -x509 -newkey rsa:2048 -sha256 -days 1 -nodes \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" \
  -subj "/CN=localhost" -addext "subjectAltName=IP:127.0.0.1,DNS:localhost" >/dev/null 2>&1

PORT=$(python3 -c "import socket;s=socket.socket();s.bind(('127.0.0.1',0));print(s.getsockname()[1]);s.close()")
DB="${DATABASE_URL:-postgres://brighto_router:brighto_router_dev@127.0.0.1:55432/brighto_router}"

TLS_CERT_PATH="$TMP/cert.pem" TLS_KEY_PATH="$TMP/key.pem" DATABASE_URL="$DB" \
  LISTEN_ADDR="127.0.0.1:$PORT" ADMIN_MASTER_KEY="tls-smoke" RUST_LOG=info \
  "$BIN" >"$TMP/log" 2>&1 &
RPID=$!

ok=""
for _ in $(seq 1 40); do
  if curl -sk -m 2 "https://127.0.0.1:$PORT/healthz" 2>/dev/null | grep -q ok; then ok=1; break; fi
  sleep 0.5
done
[ -n "$ok" ] || { echo "FAIL: https healthz"; tail -20 "$TMP/log"; exit 1; }

if curl -s -m 3 "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1; then
  echo "FAIL: plain HTTP unexpectedly answered on TLS port"; tail -20 "$TMP/log"; exit 1
fi

grep -q 'protocol":"https' "$TMP/log" || { echo "FAIL: no protocol=https log"; tail -20 "$TMP/log"; exit 1; }

if TLS_CERT_PATH="$TMP/cert.pem" TLS_KEY_PATH="$TMP/key.pem" LISTEN_ADDR="127.0.0.1:$PORT" "$BIN" healthcheck; then
  echo "PASS healthcheck subcommand under TLS"
else
  echo "FAIL: healthcheck subcommand under TLS"; exit 1
fi

echo "PASS tls smoke"
