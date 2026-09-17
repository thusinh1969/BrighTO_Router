#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${BRIGHTO_PW_OUT:-$ROOT/swarm/out/playwright/${STAMP}-portal-logic-acceptance}"
BASE_URL="${BRIGHTO_BASE_URL:-https://127.0.0.1:18443}"
mkdir -p "$OUT_DIR"

if [[ -z "${BRIGHTO_ADMIN_KEY:-}" ]]; then
  if [[ -f "$ROOT/.env" ]]; then
    BRIGHTO_ADMIN_KEY="$(awk -F= '$1=="ADMIN_MASTER_KEY"{print substr($0, index($0,"=")+1)}' "$ROOT/.env" | tail -1)"
  fi
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
  python3 - <<'PY' || true
import os, requests, urllib3
urllib3.disable_warnings()
root=os.environ['ROOT']; base=os.environ['BASE_URL']; admin=os.environ['BRIGHTO_ADMIN_KEY']
h={'x-admin-key':admin,'content-type':'application/json'}
try:
    routes=requests.get(base+'/admin/routes',headers=h,verify=False,timeout=10).json()
    for r in routes:
        if str(r.get('model_name','')).startswith(('pw-','crud-','verify-')):
            requests.delete(base+'/admin/routes/'+requests.utils.quote(r['model_name'],safe=''),headers=h,verify=False,timeout=10)
    backends=requests.get(base+'/admin/backends',headers=h,verify=False,timeout=10).json()
    routes=requests.get(base+'/admin/routes',headers=h,verify=False,timeout=10).json()
    used={bid for rr in routes for bid in rr.get('backend_ids',[])} | {rr.get('fallback_backend_id') for rr in routes if rr.get('fallback_backend_id') is not None}
    for b in backends:
        if str(b.get('name','')).startswith(('pw-','crud-','verify-')) and b.get('id') not in used:
            requests.delete(base+f"/admin/backends/{b['id']}",headers=h,verify=False,timeout=10)
except Exception as e:
    print('API cleanup warning:',e)
PY
  if command -v docker >/dev/null 2>&1 && [[ -f "$ROOT/docker-compose.yml" ]]; then
    db_cid="$(cd "$ROOT" && docker compose ps -q postgres 2>/dev/null || true)"
    if [[ -n "$db_cid" ]]; then
      docker exec -i "$db_cid" psql -U "${DB_USER:-brighto_router}" -d "${DB_NAME:-brighto_router}" >/dev/null 2>&1 <<'SQL' || true
DELETE FROM api_keys WHERE owner LIKE 'pw-%' OR owner LIKE 'crud-%' OR owner LIKE 'verify-%';
DELETE FROM teams WHERE name LIKE 'pw-%' OR name LIKE 'crud-%' OR name LIKE 'verify-%';
DELETE FROM usage_ledger WHERE model LIKE 'pw-%' OR model LIKE 'crud-%' OR model LIKE 'verify-%';
DELETE FROM model_routes WHERE model_name LIKE 'pw-%' OR model_name LIKE 'crud-%' OR model_name LIKE 'verify-%';
DELETE FROM backends WHERE base_url = 'http://127.0.0.1:9000/v1';
SQL
    fi
  fi
}

export ROOT BASE_URL BRIGHTO_ADMIN_KEY
cleanup_test_records
cp "$ROOT/swarm/scripts/portal_logic_acceptance.mjs" "$OUT_DIR/portal_logic_acceptance.mjs"

run_local_gate() {
  cd "$OUT_DIR"
  if [[ ! -d node_modules/playwright ]]; then
    npm init -y >/dev/null
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm install playwright@1.63.0 --no-audit --no-fund >/dev/null
  fi
  export BRIGHTO_BASE_URL="$BASE_URL"
  export BRIGHTO_PW_OUT="$OUT_DIR"
  export NODE_TLS_REJECT_UNAUTHORIZED=0
  node portal_logic_acceptance.mjs
}

run_docker_gate() {
  docker run --rm --network host \
    -e BRIGHTO_BASE_URL="$BASE_URL" \
    -e BRIGHTO_ADMIN_KEY="$BRIGHTO_ADMIN_KEY" \
    -e BRIGHTO_PW_OUT=/work \
    -e NODE_TLS_REJECT_UNAUTHORIZED=0 \
    -e PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 \
    -e PLAYWRIGHT_CHROMIUM_EXECUTABLE=/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-linux64/chrome-headless-shell \
    -v "$OUT_DIR:/work" \
    mcr.microsoft.com/playwright:v1.63.0-noble \
    bash -lc 'cd /work && npm init -y >/dev/null && npm install playwright@1.63.0 --no-audit --no-fund >/dev/null && node portal_logic_acceptance.mjs'
}

set +e
if [[ "${BRIGHTO_PLAYWRIGHT_LOCAL:-0}" == "1" ]]; then
  run_local_gate
else
  run_docker_gate
fi
rc=$?
set -e
cleanup_test_records
exit "$rc"
