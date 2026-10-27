#!/usr/bin/env bash
# Isolated UAT stack for the single local Qwen 3.8 model, exposed as both
# OpenAI Chat and Anthropic Messages. This script never runs production
# compose commands, never uses production ports/volumes, and never pushes an
# image. The UAT router is bound to 0.0.0.0:18081 for LAN access.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UAT_DIR="$ROOT/uat"
ENV_FILE="$UAT_DIR/.env.uat"
PROJECT="brighto-uat"

say() { printf '==> %s\n' "$*" >&2; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

command -v docker >/dev/null 2>&1 || fail "docker is required"
docker compose version >/dev/null 2>&1 || fail "docker compose v2 is required"
command -v python3 >/dev/null 2>&1 || fail "python3 is required"

compose() {
  docker compose -p "$PROJECT" -f "$UAT_DIR/docker-compose.yml" --env-file "$ENV_FILE" "$@"
}

random_hex() {
  python3 -c 'import secrets,sys; print(secrets.token_hex(int(sys.argv[1])))' "$1"
}

set_env_value() {
  local key="$1" value="$2"
  python3 - "$ENV_FILE" "$key" "$value" <<'PY'
import pathlib, sys
path, key, value = sys.argv[1], sys.argv[2], sys.argv[3]
p = pathlib.Path(path)
lines = p.read_text().splitlines()
out = []
replaced = False
for line in lines:
    if line.startswith(key + "="):
        out.append(f"{key}={value}")
        replaced = True
    else:
        out.append(line)
if not replaced:
    out.append(f"{key}={value}")
p.write_text("\n".join(out) + "\n")
PY
}

prepare_tls() {
  local mode="${UAT_TLS:-auto}"
  if [[ "$mode" == "0" ]]; then
    UAT_TLS_CERT_PATH=""
    UAT_TLS_KEY_PATH=""
    UAT_CA_CERT=""
    UAT_BASE_URL="http://127.0.0.1:18081"
  elif [[ -f "$ROOT/ssl/fullchain.pem" && -f "$ROOT/ssl/privkey.pem" ]]; then
    mkdir -p "$UAT_DIR/tls"
    cp "$ROOT/ssl/fullchain.pem" "$UAT_DIR/tls/fullchain.pem"
    cp "$ROOT/ssl/privkey.pem" "$UAT_DIR/tls/privkey.pem"
    chmod 600 "$UAT_DIR/tls/privkey.pem"
    if command -v setfacl >/dev/null 2>&1 && setfacl -m u:10001:r "$UAT_DIR/tls/privkey.pem" 2>/dev/null; then
      :
    else
      chmod 644 "$UAT_DIR/tls/privkey.pem"
      say "WARN: setfacl unavailable; UAT TLS key is 0644 (UAT copy only)"
    fi
    UAT_TLS_CERT_PATH="/certs/fullchain.pem"
    UAT_TLS_KEY_PATH="/certs/privkey.pem"
    UAT_CA_CERT="$UAT_DIR/tls/fullchain.pem"
    UAT_BASE_URL="https://127.0.0.1:18081"
  else
    UAT_TLS_CERT_PATH=""
    UAT_TLS_KEY_PATH=""
    UAT_CA_CERT=""
    UAT_BASE_URL="http://127.0.0.1:18081"
    say "WARN: ssl/fullchain.pem or ssl/privkey.pem missing; UAT runs plain HTTP"
  fi
  export UAT_TLS_CERT_PATH UAT_TLS_KEY_PATH UAT_CA_CERT UAT_BASE_URL
  set_env_value UAT_BASE_URL "$UAT_BASE_URL"
  set_env_value UAT_CA_CERT "$UAT_CA_CERT"
  set_env_value UAT_TLS_CERT_PATH "$UAT_TLS_CERT_PATH"
  set_env_value UAT_TLS_KEY_PATH "$UAT_TLS_KEY_PATH"
}

router_curl() {
  if [[ -n "${UAT_CA_CERT:-}" && -f "$UAT_CA_CERT" ]]; then
    curl --cacert "$UAT_CA_CERT" "$@"
  else
    curl "$@"
  fi
}

ensure_env() {
  if [[ ! -f "$ENV_FILE" ]]; then
    say "Creating $ENV_FILE from uat/.env.example"
    cp "$UAT_DIR/.env.example" "$ENV_FILE"
  fi
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
  prepare_tls

  if [[ -z "${ADMIN_MASTER_KEY:-}" || "$ADMIN_MASTER_KEY" == replace-* || "$ADMIN_MASTER_KEY" == change-me* ]]; then
    ADMIN_MASTER_KEY="$(random_hex 32)"
    set_env_value ADMIN_MASTER_KEY "$ADMIN_MASTER_KEY"
    say "Generated ADMIN_MASTER_KEY for UAT (stored in uat/.env.uat)"
  fi
  if [[ -z "${UAT_CLIENT_KEY:-}" || "$UAT_CLIENT_KEY" == sk-brighto-uat-replace-* ]]; then
    UAT_CLIENT_KEY="sk-brighto-uat-$(random_hex 24)"
    set_env_value UAT_CLIENT_KEY "$UAT_CLIENT_KEY"
    say "Generated UAT client key (stored in uat/.env.uat)"
  fi
  BRIGHTO_ROUTER_IMAGE="${BRIGHTO_ROUTER_IMAGE:-thusinh1969/brighto_airouter:uat-qwen-dual}"
  export BRIGHTO_ROUTER_IMAGE
  case "$BRIGHTO_ROUTER_IMAGE" in
    thusinh1969/brighto_airouter:v1.2.0|thusinh1969/brighto_airouter:v1.3.0|thusinh1969/brighto_airouter:latest|thusinh1969/brighto_airouter:main)
      fail "UAT must not run a production image tag: $BRIGHTO_ROUTER_IMAGE"
      ;;
  esac
  UAT_BASE_URL="${UAT_BASE_URL:-https://127.0.0.1:18081}"
  UAT_QWEN_BASE_URL="${UAT_QWEN_BASE_URL:-http://host.docker.internal:8088/v1}"
  UAT_QWEN_MODEL="${UAT_QWEN_MODEL:-qwen3.8-flash-next}"
  UAT_MODEL="$UAT_QWEN_MODEL"
  export UAT_BASE_URL UAT_MODEL UAT_QWEN_BASE_URL UAT_QWEN_MODEL

  mkdir -p "$UAT_DIR/work"
  printf 'claude-bridge-uat\n' > "$UAT_DIR/work/uat.txt"

  say "UAT project=$PROJECT router=$UAT_BASE_URL model=$UAT_QWEN_MODEL db=127.0.0.1:55433"
}

build_image() {
  ensure_env
  say "Building isolated UAT image $BRIGHTO_ROUTER_IMAGE (no push)"
  docker build -t "$BRIGHTO_ROUTER_IMAGE" "$ROOT"
}

wait_for_pg() {
  for _ in $(seq 1 60); do
    if compose exec -T postgres pg_isready -U brighto_router -d brighto_router >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  fail "UAT PostgreSQL did not become ready"
}

psql_uat() {
  compose exec -T -e "PGPASSWORD=${DB_PASS:-brighto_router_uat}" postgres \
    psql -U brighto_router -d brighto_router -v ON_ERROR_STOP=1 "$@"
}

run_migrations() {
  say "Applying UAT migrations (tracked in uat_schema_migrations)"
  psql_uat -c "CREATE TABLE IF NOT EXISTS uat_schema_migrations (name TEXT PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT now())" >/dev/null
  local migration base applied
  for migration in "$ROOT"/migrations/*.sql; do
    base="$(basename "$migration")"
    applied="$(psql_uat -tAc "SELECT 1 FROM uat_schema_migrations WHERE name = '$base'")"
    if [[ "$applied" == "1" ]]; then
      continue
    fi
    say "  applying $base"
    psql_uat -f "/migrations/$base" >/dev/null
    psql_uat -c "INSERT INTO uat_schema_migrations(name) VALUES ('$base')" >/dev/null
  done
}

seed_uat() {
  say "Seeding UAT team, client key, and the single Qwen 3.8 dual-protocol route"
  python3 "$UAT_DIR/seed.py" | psql_uat >/dev/null
}

wait_for_router() {
  for _ in $(seq 1 60); do
    if router_curl -fsS "$UAT_BASE_URL/readyz" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  fail "UAT router did not become ready; inspect: $0 logs"
}

cmd_up() {
  build_image
  compose up -d postgres
  wait_for_pg
  run_migrations
  seed_uat
  compose up -d router
  wait_for_router
  cmd_status
  printf '\nUAT admin key and client key are in %s\n' "$ENV_FILE"
  printf 'Run: ./uat/start.sh test    # Qwen dual-protocol API checks\n'
  printf 'Run: ./uat/start.sh portal  # Portal browser check\n'
  printf 'Run: ./uat/start.sh claude  # real Claude CLI text + tool round trip\n'
  printf 'Run: ./uat/start.sh qwen    # full Qwen API + Claude CLI check\n'
}

cmd_status() {
  ensure_env
  compose ps
  printf 'router /healthz: %s\n' "$(router_curl -sS -o /dev/null -w '%{http_code}' "$UAT_BASE_URL/healthz" || true)"
  printf 'router /readyz:  %s\n' "$(router_curl -sS -o /dev/null -w '%{http_code}' "$UAT_BASE_URL/readyz" || true)"
  printf 'Portal:          %s/\n' "$UAT_BASE_URL"
}

cmd_down() {
  ensure_env
  compose down --remove-orphans
}

cmd_reset() {
  ensure_env
  say "Removing only the brighto-uat project containers and named volumes"
  compose down -v --remove-orphans
  rm -rf "$UAT_DIR/claude-config"
}

cmd_test() {
  ensure_env
  python3 "$UAT_DIR/check_qwen.py"
}

cmd_portal() {
  ensure_env
  python3 "$UAT_DIR/portal_bridge_check.py"
}

cmd_claude() {
  ensure_env
  "$UAT_DIR/claude_cli_e2e.sh"
}

cmd_qwen() {
  ensure_env
  python3 "$UAT_DIR/check_qwen.py"
  "$UAT_DIR/claude_cli_e2e.sh"
}

cmd_env() {
  ensure_env
  printf 'UAT_BASE_URL=%s\n' "$UAT_BASE_URL"
  printf 'UAT_MODEL=%s\n' "$UAT_MODEL"
  printf 'UAT_QWEN_MODEL=%s\n' "$UAT_QWEN_MODEL"
  printf 'UAT_QWEN_BASE_URL=%s\n' "$UAT_QWEN_BASE_URL"
  printf 'UAT_CLIENT_KEY=%s\n' "$UAT_CLIENT_KEY"
  printf 'ADMIN_MASTER_KEY=%s\n' "$ADMIN_MASTER_KEY"
}

case "${1:-up}" in
  up) cmd_up ;;
  build) build_image ;;
  down) cmd_down ;;
  reset) cmd_reset ;;
  restart) cmd_down; cmd_up ;;
  status) cmd_status ;;
  logs) ensure_env; compose logs -f ;;
  migrate) ensure_env; compose up -d postgres; wait_for_pg; run_migrations ;;
  seed) ensure_env; seed_uat ;;
  test) cmd_test ;;
  portal) cmd_portal ;;
  claude) cmd_claude ;;
  qwen) cmd_qwen ;;
  env) cmd_env ;;
  *)
    cat >&2 <<EOF
Usage: $0 {up|build|down|reset|restart|status|logs|migrate|seed|test|portal|claude|qwen|env}

This is the isolated UAT stack (project brighto-uat). The router listens on
0.0.0.0:18081 for LAN access; PostgreSQL stays on loopback 55433. It does
not touch production ports, database, volumes, or Docker Hub.
EOF
    exit 2
    ;;
esac
