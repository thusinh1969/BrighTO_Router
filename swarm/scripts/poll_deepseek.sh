#!/usr/bin/env bash
set -euo pipefail

ROOT="${BRIGHTO_REPO:-/mnt/data02/BrigTO_Router}"
INTERVAL="${1:-600}"
LOG="$ROOT/swarm/out/audit-poller/poller.log"
mkdir -p "$(dirname "$LOG")"
cd "$ROOT"

last=""
redact() {
  sed -E 's/(sk-[A-Za-z0-9_-]{12,})/[REDACTED_KEY]/g; s/([A-Za-z0-9_]{20,}\.[A-Za-z0-9_.-]{20,})/[REDACTED_TOKEN]/g; s/(ADMIN_MASTER_KEY=).+$/\1[REDACTED]/g'
}

fingerprint() {
  {
    git status --short --branch
    git rev-parse HEAD
    git diff --stat
    find swarm/audits -maxdepth 1 -type f -printf '%T@ %f\n' 2>/dev/null | sort
    if [[ -f static/index.html ]]; then
      stat -c '%Y %s' static/index.html
      sha256sum static/index.html
    fi
  } | sha256sum | awk '{print $1}'
}

portal_static_gate() {
  python3 - <<'PY'
from pathlib import Path
import re
p = Path('static/index.html')
text = p.read_text() if p.exists() else ''
checks = []

def check(name, ok, evidence=''):
    checks.append((name, ok, evidence))

check('has Portal preferences settings', 'Portal preferences' in text, 'missing marker')
check('has font-size preference state', ('data-font' in text or 'dataset.font' in text) and re.search(r'Small|Normal|Large', text, re.I), 'missing data-font/Small/Normal/Large')
check('has density preference state', ('data-density' in text or 'dataset.density' in text) and re.search(r'Compact|Comfortable', text, re.I), 'missing data-density/Compact/Comfortable')
check('has compact count formatter', ('fmtCount' in text or 'formatCompact' in text), 'missing fmtCount/formatCompact')
check('chart formatter uses uppercase K', not re.search(r'\+"k"|\+\'k\'|"k"\s*;', text), 'lowercase k marker present')
for fn in ['renderProviders', 'renderModels', 'renderTeams', 'renderKeys', 'renderUsage']:
    pattern = f'{fn}($("content"))'
    check(f'no direct post-action {fn} append', pattern not in text, pattern)
failures = [c for c in checks if not c[1]]
print('PORTAL_STATIC_GATE', 'PASS' if not failures else f'FAIL {len(failures)}')
for name, ok, evidence in checks:
    print(('PASS ' if ok else 'FAIL ') + name + ('' if ok else f' :: {evidence}'))
PY
}

echo "[$(date -Is)] audit poller started interval=${INTERVAL}s pid=$$" >> "$LOG"
while true; do
  fp="$(fingerprint)"
  if [[ "$fp" != "$last" ]]; then
    {
      echo "[$(date -Is)] CHANGE fp=$fp"
      echo '[git status]'
      git status --short --branch
      echo '[git log]'
      git log --oneline -12 --decorate
      echo '[diffstat]'
      git diff --stat
      echo '[newest audits]'
      find swarm/audits -maxdepth 1 -type f -printf '%T@ %f\n' 2>/dev/null | sort -nr | head -24
      echo '[cargo check]'
      cargo check --locked --all-targets 2>&1 || true
      echo '[portal static gate]'
      portal_static_gate || true
      echo '[portal runtime health]'
      curl -ksS --max-time 5 -w '\nHTTP %{http_code}\n' https://127.0.0.1:18443/healthz || true
      echo '[portal js endpoints]'
      python3 - <<'PY'
from pathlib import Path
import re
p = Path('static/index.html')
text = p.read_text() if p.exists() else ''
print('static/index.html bytes', len(text))
print('fetch endpoints', ', '.join(sorted(set(re.findall(r"['\"](/(?:admin|portal|v1)/[^'\"]*)['\"]", text)))))
PY
      echo '[done]'
    } | redact >> "$LOG"
    last="$fp"
  fi
  sleep "$INTERVAL" & wait $!
done
