#!/usr/bin/env bash
set -euo pipefail

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SELF_DIR/../.." && pwd)"
AUD="$REPO/swarm/audits"
OUT_DIR="$REPO/swarm/out"
OUT="$OUT_DIR/audit_verdicts.md"
LOG="$AUD/WATCH.log"
SEEN="$OUT_DIR/.audit_seen"

mkdir -p "$OUT_DIR"
touch "$SEEN"

shopt -s nullglob
for file in "$AUD"/*; do
  [[ -f "$file" ]] || continue
  base="$(basename "$file")"
  case "$base" in
    WATCH.log|watch_noop.log|README.md|AUDITOR-POLL-STATE.md) continue ;;
  esac

  fp="$(stat -c '%Y %s' "$file") $(sha256sum "$file" | cut -d' ' -f1)"
  key="$base|$fp"
  if ! grep -qFx "$key" "$SEEN"; then
    {
      printf '%s NEW_OR_CHANGED_AUDIT %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$base"
    } >> "$LOG"
    {
      printf '\n## %s — %s\n\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$base"
      cat "$file"
      printf '\n'
    } >> "$OUT"
    printf '%s\n' "$key" >> "$SEEN"
  fi
done
