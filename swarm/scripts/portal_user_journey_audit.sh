#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${BRIGHTO_PW_OUT:-$ROOT/swarm/out/playwright/${STAMP}-portal-user-journey-audit}"
BASE_URL="${BRIGHTO_BASE_URL:-https://127.0.0.1:18443}"
mkdir -p "$OUT_DIR"
if [[ -z "${BRIGHTO_ADMIN_KEY:-}" && -f "$ROOT/.env" ]]; then
  BRIGHTO_ADMIN_KEY="$(awk -F= '$1=="ADMIN_MASTER_KEY"{print substr($0, index($0,"=")+1)}' "$ROOT/.env" | tail -1)"
fi
if [[ -z "${BRIGHTO_ADMIN_KEY:-}" ]]; then
  echo "BRIGHTO_ADMIN_KEY is required or ADMIN_MASTER_KEY must exist in .env" >&2
  exit 2
fi
cleanup_test_records() {
  if command -v docker >/dev/null 2>&1 && [[ -f "$ROOT/docker-compose.yml" ]]; then
    db_cid="$(cd "$ROOT" && docker compose ps -q postgres 2>/dev/null || true)"
    if [[ -n "$db_cid" ]]; then
      docker exec -i "$db_cid" psql -U "${DB_USER:-brighto_router}" -d "${DB_NAME:-brighto_router}" >/dev/null 2>&1 <<'SQL' || true
DELETE FROM api_keys WHERE owner LIKE 'pw-user-%';
DELETE FROM teams WHERE name LIKE 'pw-user-%';
DELETE FROM usage_ledger WHERE model LIKE 'pw-user-%';
DELETE FROM model_routes WHERE model_name LIKE 'pw-user-%';
DELETE FROM backends WHERE name LIKE 'pw-user-%';
SQL
    fi
  fi
}
trap cleanup_test_records EXIT
cleanup_test_records
cp "$ROOT/swarm/scripts/portal_user_journey_audit.mjs" "$OUT_DIR/portal_user_journey_audit.mjs"
docker run --rm --network host \
  -e BRIGHTO_BASE_URL="$BASE_URL" \
  -e BRIGHTO_ADMIN_KEY="$BRIGHTO_ADMIN_KEY" \
  -e BRIGHTO_PW_OUT=/work \
  -e NODE_TLS_REJECT_UNAUTHORIZED=0 \
  -e PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 \
  -e PLAYWRIGHT_CHROMIUM_EXECUTABLE=/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-linux64/chrome-headless-shell \
  -v "$OUT_DIR:/work" \
  mcr.microsoft.com/playwright:v1.63.0-noble \
  bash -lc 'cd /work && npm init -y >/dev/null && npm install playwright@1.63.0 --no-audit --no-fund >/dev/null && node portal_user_journey_audit.mjs'
