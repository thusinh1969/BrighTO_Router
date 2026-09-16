#!/usr/bin/env bash
# Compatibility entrypoint for BENCHMARK.md tier-B release gates.
# Canonical implementation lives in scripts/bench_real.py. Keep one benchmark truth only.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

# Backward compatibility with the old shell gate env name.
if [[ -n "${CONC:-}" && -z "${CONCS:-}" ]]; then
  export CONCS="$CONC"
fi

# gate.sh is a release gate by default; focused smoke runs may override REQUIRE_PASS=0.
export REQUIRE_PASS="${REQUIRE_PASS:-1}"
exec python3 scripts/bench_real.py "$@"
