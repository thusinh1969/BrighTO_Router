#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${BRIGHTO_PW_OUT:-$ROOT/swarm/out/playwright/${STAMP}-portal-modal-surface-audit}"
BASE_URL="${BRIGHTO_BASE_URL:-https://127.0.0.1:18443}"
mkdir -p "$OUT_DIR"
if [[ -z "${BRIGHTO_ADMIN_KEY:-}" && -f "$ROOT/.env" ]]; then
  BRIGHTO_ADMIN_KEY="$(awk -F= '$1=="ADMIN_MASTER_KEY"{print substr($0, index($0,"=")+1)}' "$ROOT/.env" | tail -1)"
fi
if [[ -z "${BRIGHTO_ADMIN_KEY:-}" ]]; then
  echo "BRIGHTO_ADMIN_KEY is required or ADMIN_MASTER_KEY must exist in .env" >&2
  exit 2
fi
cp "$ROOT/swarm/scripts/portal_modal_surface_audit.mjs" "$OUT_DIR/portal_modal_surface_audit.mjs"
docker run --rm --network host \
  -e BRIGHTO_BASE_URL="$BASE_URL" \
  -e BRIGHTO_ADMIN_KEY="$BRIGHTO_ADMIN_KEY" \
  -e BRIGHTO_PW_OUT=/work \
  -e NODE_TLS_REJECT_UNAUTHORIZED=0 \
  -e PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 \
  -e PLAYWRIGHT_CHROMIUM_EXECUTABLE=/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-linux64/chrome-headless-shell \
  -v "$OUT_DIR:/work" \
  mcr.microsoft.com/playwright:v1.63.0-noble \
  bash -lc 'cd /work && npm init -y >/dev/null && npm install playwright@1.63.0 --no-audit --no-fund >/dev/null && node portal_modal_surface_audit.mjs'
