#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${BRIGHTO_PW_OUT:-$ROOT/swarm/out/playwright/${STAMP}-portal-login-audit}"
BASE_URL="${BRIGHTO_BASE_URL:-https://127.0.0.1:18443}"
mkdir -p "$OUT_DIR"
if [[ -z "${PLAYWRIGHT_CHROMIUM_EXECUTABLE:-}" ]]; then
  candidate="$(find "$HOME/.cache/ms-playwright" -path '*/chrome-headless-shell' -type f -perm -111 2>/dev/null | sort | tail -1 || true)"
  if [[ -n "$candidate" ]]; then export PLAYWRIGHT_CHROMIUM_EXECUTABLE="$candidate"; fi
fi
cp "$ROOT/swarm/scripts/portal_login_audit.mjs" "$OUT_DIR/portal_login_audit.mjs"
cd "$OUT_DIR"
if [[ ! -d node_modules/playwright ]]; then
  npm init -y >/dev/null
  npm install playwright --no-audit --no-fund >/dev/null
fi
export BRIGHTO_BASE_URL="$BASE_URL" BRIGHTO_PW_OUT="$OUT_DIR" NODE_TLS_REJECT_UNAUTHORIZED=0
node portal_login_audit.mjs
