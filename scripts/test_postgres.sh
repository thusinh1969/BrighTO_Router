#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DEFAULT_DATABASE_URL="postgres://brighto_router:brighto_router_dev@127.0.0.1:55432/brighto_router"
# Legacy default is recognized only so tests do not use an old local development DB by accident.
OLD_DEFAULT_DATABASE_URL="postgres://brighto_router:brighto_router_dev@127.0.0.1:5432/brighto_router"
DATABASE_URL_WAS_SET=0
TEST_DATABASE_URL_WAS_SET=0
[[ -n "${DATABASE_URL+x}" ]] && DATABASE_URL_WAS_SET=1
[[ -n "${TEST_DATABASE_URL+x}" ]] && TEST_DATABASE_URL_WAS_SET=1
DATABASE_URL="${DATABASE_URL:-$DEFAULT_DATABASE_URL}"
TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"
EXPLICIT_TEST_DB=0
if [[ "$TEST_DATABASE_URL_WAS_SET" == "1" ]]; then
  EXPLICIT_TEST_DB=1
elif [[ "$DATABASE_URL_WAS_SET" == "1" && "$DATABASE_URL" != "$DEFAULT_DATABASE_URL" && "$DATABASE_URL" != "$OLD_DEFAULT_DATABASE_URL" ]]; then
  EXPLICIT_TEST_DB=1
fi
STARTED_TEST_PG=""

say() { printf '==> %s\n' "$*" >&2; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

can_connect() {
  command -v psql >/dev/null 2>&1 && psql "$1" -c 'select 1' >/dev/null 2>&1
}

start_temp_postgres() {
  command -v docker >/dev/null 2>&1 || fail "Postgres is not reachable at DATABASE_URL=$TEST_DATABASE_URL and docker is not installed"
  STARTED_TEST_PG="brighto_test_pg_$$"
  say "Starting temporary Postgres for tests: $STARTED_TEST_PG"
  docker run --rm -d --name "$STARTED_TEST_PG" \
    -e POSTGRES_DB=brighto_router \
    -e POSTGRES_USER=brighto_router \
    -e POSTGRES_PASSWORD=brighto_router_dev \
    -p 127.0.0.1::5432 \
    postgres:16-alpine >/dev/null
  local port
  port="$(docker port "$STARTED_TEST_PG" 5432/tcp | awk -F: '{print $NF}')"
  TEST_DATABASE_URL="postgres://brighto_router:brighto_router_dev@127.0.0.1:${port}/brighto_router"
  for _ in $(seq 1 60); do
    if docker exec "$STARTED_TEST_PG" pg_isready -U brighto_router -d brighto_router >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  fail "temporary Postgres did not become ready"
}

cleanup() {
  if [[ -n "$STARTED_TEST_PG" ]]; then
    docker stop -t 5 "$STARTED_TEST_PG" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ "$EXPLICIT_TEST_DB" == "0" ]]; then
  start_temp_postgres
elif ! can_connect "$TEST_DATABASE_URL"; then
  fail "Postgres is not reachable at DATABASE_URL=$TEST_DATABASE_URL"
fi

export DATABASE_URL="$TEST_DATABASE_URL"

if command -v sqlx >/dev/null 2>&1; then
  sqlx migrate run >/dev/null
else
  command -v psql >/dev/null 2>&1 || fail "psql or sqlx is required to run migrations"
  for migration in migrations/*.sql; do
    [[ -e "$migration" ]] || fail "no migrations found"
    psql "$DATABASE_URL" -v ON_ERROR_STOP=1 < "$migration" >/dev/null
  done
fi

psql "$DATABASE_URL" -v ON_ERROR_STOP=1 >/dev/null <<'SQL'
BEGIN;
CREATE TEMP TABLE teams (name TEXT, budget TEXT, enabled BOOLEAN);
CREATE TEMP TABLE backends (name TEXT, base_url TEXT, api_key_ref TEXT,
                            weight BIGINT, max_inflight BIGINT, format TEXT, enabled BOOLEAN);
\i scripts/seed_defaults.sql
\i scripts/seed_defaults.sql
DO $$ BEGIN
  IF (SELECT COUNT(*) FROM teams WHERE name = 'Default Team' AND budget IS NULL) <> 1 THEN
    RAISE EXCEPTION 'fresh Default Team must be unlimited and seeding must be idempotent';
  END IF;
END $$;
UPDATE teams SET budget = '{"period":"month","max_tokens":50,"per_model":{}}'
WHERE name = 'Default Team';
\i scripts/seed_defaults.sql
DO $$ BEGIN
  IF (SELECT budget FROM teams WHERE name = 'Default Team') NOT LIKE '%"max_tokens":50%' THEN
    RAISE EXCEPTION 'seeding must preserve an existing team budget';
  END IF;
END $$;
ROLLBACK;
SQL

CARGO_INCREMENTAL=0 cargo test --locked --all-targets
