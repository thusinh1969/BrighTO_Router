#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${BRIGHTO_PW_OUT:-$ROOT/swarm/out/playwright/${STAMP}-portal-full-page-audit}"
BASE_URL="${BRIGHTO_BASE_URL:-https://127.0.0.1:18443}"
mkdir -p "$OUT_DIR"
if [[ -z "${BRIGHTO_ADMIN_KEY:-}" && -f "$ROOT/.env" ]]; then
  BRIGHTO_ADMIN_KEY="$(awk -F= '$1=="ADMIN_MASTER_KEY"{print substr($0, index($0,"=")+1)}' "$ROOT/.env" | tail -1)"
fi
if [[ -z "${BRIGHTO_ADMIN_KEY:-}" ]]; then
  echo "BRIGHTO_ADMIN_KEY is required or ADMIN_MASTER_KEY must exist in .env" >&2
  exit 2
fi
if [[ -z "${PLAYWRIGHT_CHROMIUM_EXECUTABLE:-}" ]]; then
  candidate="$(find "$HOME/.cache/ms-playwright" -path '*/chrome-headless-shell' -type f -perm -111 2>/dev/null | sort | tail -1 || true)"
  if [[ -n "$candidate" ]]; then export PLAYWRIGHT_CHROMIUM_EXECUTABLE="$candidate"; fi
fi
cleanup_test_records() {
  if command -v docker >/dev/null 2>&1 && [[ -f "$ROOT/docker-compose.yml" ]]; then
    db_cid="$(cd "$ROOT" && docker compose ps -q postgres 2>/dev/null || true)"
    if [[ -n "$db_cid" ]]; then
      docker exec -i "$db_cid" psql -U "${DB_USER:-brighto_router}" -d "${DB_NAME:-brighto_router}" >/dev/null 2>&1 <<'SQL' || true
DELETE FROM api_keys WHERE owner LIKE 'pw-fullvisual-%';
DELETE FROM teams WHERE name LIKE 'pw-fullvisual-%';
DELETE FROM usage_ledger WHERE model LIKE 'pw-fullvisual-%';
DELETE FROM model_routes WHERE model_name LIKE 'pw-fullvisual-%';
DELETE FROM backends WHERE name LIKE 'pw-fullvisual-%';
SQL
    fi
  fi
}
trap cleanup_test_records EXIT
cleanup_test_records
cp "$ROOT/swarm/scripts/portal_full_page_audit.mjs" "$OUT_DIR/portal_full_page_audit.mjs"
cd "$OUT_DIR"
if [[ ! -d node_modules/playwright ]]; then
  npm init -y >/dev/null
  npm install playwright --no-audit --no-fund >/dev/null
fi
export BRIGHTO_BASE_URL="$BASE_URL" BRIGHTO_ADMIN_KEY BRIGHTO_PW_OUT="$OUT_DIR" NODE_TLS_REJECT_UNAUTHORIZED=0
node portal_full_page_audit.mjs
