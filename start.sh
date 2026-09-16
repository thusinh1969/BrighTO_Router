#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

COMPOSE="${COMPOSE:-docker compose}"
COMPOSE_FILE_PATH="${COMPOSE_FILE_PATH:-$ROOT/docker-compose.yml}"
ENV_FILE="$ROOT/.env"
DEFAULT_URL="postgres://brighto_router:brighto_router_dev@127.0.0.1:5432/brighto_router"

say() { printf '\n==> %s\n' "$*"; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

ensure_env() {
  if [[ ! -f "$ENV_FILE" ]]; then
    [[ -f .env.example ]] || fail ".env is missing and .env.example was not found"
    cp .env.example .env
    chmod 600 .env
    cat >&2 <<'MSG'
Created .env from .env.example.
The default ADMIN_MASTER_KEY is for local development only. Change it before production.
MSG
  fi
}

load_env() {
  ensure_env
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
  DATABASE_URL_EFFECTIVE="${DATABASE_URL:-$DEFAULT_URL}"
  DB_HOST="${DB_HOST:-127.0.0.1}"
  DB_PORT="${DB_PORT:-5432}"
  DB_NAME="${DB_NAME:-brighto_router}"
  DB_USER="${DB_USER:-brighto_router}"
  DB_PASS="${DB_PASS:-brighto_router_dev}"
  LISTEN_ADDR_EFFECTIVE="${LISTEN_ADDR:-0.0.0.0:8080}"
  HEALTH_HOST="${HEALTH_HOST:-127.0.0.1}"
  HEALTH_PORT="${LISTEN_ADDR_EFFECTIVE##*:}"
  BASE_URL="${BASE_URL:-http://${HEALTH_HOST}:${HEALTH_PORT}}"
}

compose() {
  [[ -f "$COMPOSE_FILE_PATH" ]] || fail "docker-compose.yml not found at $COMPOSE_FILE_PATH"
  # shellcheck disable=SC2086
  $COMPOSE -f "$COMPOSE_FILE_PATH" "$@"
}

wait_postgres() {
  say "Waiting for Postgres"
  for _ in $(seq 1 60); do
    if compose exec -T postgres pg_isready -U "$DB_USER" -d "$DB_NAME" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  fail "Postgres did not become ready"
}

run_migrations() {
  say "Running migrations"
  if command -v sqlx >/dev/null 2>&1; then
    DATABASE_URL="$DATABASE_URL_EFFECTIVE" sqlx migrate run
    return
  fi
  for migration in "$ROOT"/migrations/*.sql; do
    [[ -e "$migration" ]] || fail "no migrations found in $ROOT/migrations"
    compose exec -T postgres psql -v ON_ERROR_STOP=1 -U "$DB_USER" -d "$DB_NAME" < "$migration"
  done
}

validate_runtime_env() {
  if [[ -z "${ADMIN_MASTER_KEY:-}" ]]; then
    fail "ADMIN_MASTER_KEY must be set in .env"
  fi
}

health() {
  local path="$1"
  curl -fsS --max-time 2 "$BASE_URL$path" 2>/dev/null || true
}

cmd="${1:-help}"
case "$cmd" in
  start)
    load_env
    validate_runtime_env
    say "Starting Postgres"
    compose up -d postgres
    wait_postgres
    run_migrations
    say "Starting BrighTO-Router"
    compose up -d router
    say "Status"
    compose ps
    ;;
  stop)
    if [[ ! -f "$ENV_FILE" ]]; then
      say ".env is missing; no configured Compose stack to stop"
      exit 0
    fi
    say "Stopping BrighTO-Router stack"
    compose down
    ;;
  restart)
    load_env
    validate_runtime_env
    say "Restarting BrighTO-Router stack"
    compose up -d postgres
    wait_postgres
    run_migrations
    compose up -d router
    compose restart router
    compose ps
    ;;
  status)
    if [[ ! -f "$ENV_FILE" ]]; then
      say ".env is missing; run ./start.sh start to create it from .env.example"
      exit 0
    fi
    load_env
    compose ps
    printf '\nhealthz: %s\n' "$(health /healthz)"
    printf 'readyz:  %s\n' "$(health /readyz)"
    ;;
  logs)
    load_env
    compose logs -f router
    ;;
  migrate)
    load_env
    compose up -d postgres
    wait_postgres
    run_migrations
    ;;
  smoke)
    load_env
    say "Running short benchmark smoke"
    make bench-gate-smoke
    ;;
  gate)
    load_env
    say "Running release gate"
    make gate
    ;;
  build)
    say "Building release binary"
    cargo build --release --locked
    ;;
  help|-h|--help)
    cat <<'USAGE'
BrighTO-Router helper

Usage:
  ./start.sh start      Start Postgres, run migrations, pull image if needed, then start router
  ./start.sh stop       Stop the stack
  ./start.sh restart    Rebuild/restart the stack
  ./start.sh status     Show containers plus /healthz and /readyz
  ./start.sh logs       Follow router logs
  ./start.sh migrate    Run SQL migrations against DATABASE_URL
  ./start.sh smoke      Run a short non-release benchmark smoke
  ./start.sh gate       Run the release gate from Makefile
  ./start.sh build      Build target/release/brighto-router

Environment overrides:
  COMPOSE="docker compose"
  BRIGHTO_ROUTER_IMAGE=thusinh1969/brighto_airouter:v1
  DATABASE_URL=postgres://...
  DB_HOST=127.0.0.1 DB_PORT=5432 DB_NAME=brighto_router DB_USER=brighto_router DB_PASS=brighto_router_dev
  LISTEN_ADDR=0.0.0.0:8080
  BASE_URL=http://127.0.0.1:8080
  COMPOSE_FILE_PATH=/path/to/docker-compose.yml
USAGE
    ;;
  *)
    fail "unknown command: $cmd (try ./start.sh help)"
    ;;
esac
