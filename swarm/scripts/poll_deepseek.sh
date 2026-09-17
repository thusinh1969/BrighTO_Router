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
  python3 swarm/scripts/portal_static_gate.py
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
      static_status=0
      portal_static_gate || static_status=$?
      echo '[portal runtime health]'
      curl -ksS --max-time 5 -w '\nHTTP %{http_code}\n' https://127.0.0.1:18443/healthz || true
      echo '[portal runtime polish gate]'
      if [[ "$static_status" -eq 0 ]]; then
        bash swarm/scripts/portal_polish_audit.sh || true
      else
        echo "SKIP: static gate failed; fix source blockers before running browser acceptance"
      fi
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
