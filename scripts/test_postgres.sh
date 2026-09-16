#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

DEFAULT_DATABASE_URL="postgres://brighto_router:brighto_router_dev@127.0.0.1:5432/brighto_router"
DATABASE_URL="${DATABASE_URL:-$DEFAULT_DATABASE_URL}"
TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"
export DATABASE_URL="$TEST_DATABASE_URL"

if command -v psql >/dev/null 2>&1; then
  if ! psql "$DATABASE_URL" -c 'select 1' >/dev/null 2>&1; then
    cat >&2 <<MSG
ERROR: Postgres is not reachable at DATABASE_URL=$DATABASE_URL
Run ./start.sh start, or set DATABASE_URL to an existing test database.
MSG
    exit 1
  fi
else
  echo "psql not installed; skipping explicit DB reachability probe" >&2
fi

if command -v sqlx >/dev/null 2>&1; then
  sqlx migrate run >/dev/null
else
  for migration in migrations/*.sql; do
    [[ -e "$migration" ]] || { echo "ERROR: no migrations found" >&2; exit 1; }
    psql "$DATABASE_URL" -v ON_ERROR_STOP=1 < "$migration" >/dev/null
  done
fi

CARGO_INCREMENTAL=0 cargo test --locked --all-targets
