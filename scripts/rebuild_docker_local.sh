#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

IMAGE="${1:-${BRIGHTO_DEV_IMAGE:-thusinh1969/brighto_airouter:local-dev}}"
PULL_POLICY="${BRIGHTO_ROUTER_PULL_POLICY:-never}"

printf '\n==> Building Rust release binary (Portal is embedded unless PORTAL_STATIC_FILE is set)\n'
cargo build --release --locked

printf '\n==> Building Docker image: %s\n' "$IMAGE"
docker build -t "$IMAGE" .

printf '\n==> Writing local Docker override to .env\n'
if [[ ! -f .env ]]; then
  cp .env.example .env
  chmod 600 .env
fi
IMAGE="$IMAGE" python3 - <<'PY'
import os
from pathlib import Path
path = Path('.env')
updates = {
    'BRIGHTO_ROUTER_IMAGE': os.environ['IMAGE'],
    'BRIGHTO_ROUTER_PULL_POLICY': 'never',
}
lines = path.read_text().splitlines() if path.exists() else []
out = []
seen = set()
for line in lines:
    if '=' in line and not line.lstrip().startswith('#'):
        key = line.split('=', 1)[0].strip()
        if key in updates:
            out.append(f'{key}={updates[key]}')
            seen.add(key)
            continue
    out.append(line)
if out and out[-1].strip():
    out.append('')
for key, value in updates.items():
    if key not in seen:
        out.append(f'{key}={value}')
path.write_text('\n'.join(out).rstrip() + '\n')
PY

printf '\n==> Restarting router from local image\n'
BRIGHTO_ROUTER_IMAGE="$IMAGE" BRIGHTO_ROUTER_PULL_POLICY="$PULL_POLICY" docker compose up -d --no-deps --force-recreate --pull never router

printf '\n==> Runtime status\n'
./start.sh status

cat <<MSG

Local rebuild complete.
Image: $IMAGE

If you edited only Portal HTML and want instant browser refresh without rebuilding,
set PORTAL_STATIC_FILE=/app/static/index.html in .env and restart once:

  ./start.sh restart

Then edit static/index.html and refresh the browser.
MSG
