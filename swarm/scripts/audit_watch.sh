#!/bin/bash
# audit_watch.sh — cron job: poll audits/ mỗi 5 phút, bắt verdict mới/chỉnh sửa từ Auditor (CODEX/GLM).
# Fingerprint = mtime + size + sha256 => bắt cả file mới lẫn file bị SỬA (không chỉ file mới).
# Kết quả dồn vào swarm/out/audit_verdicts.md để CODER đọc gọn; log vào audits/WATCH.log.
set -u

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SELF_DIR/.." && pwd)"
AUD="$REPO/audits"
OUT="$REPO/swarm/out/audit_verdicts.md"
LOG="$AUD/WATCH.log"
SEEN="$REPO/swarm/out/.audit_seen"

mkdir -p "$REPO/swarm/out"
touch "$SEEN" 2>/dev/null || true

shopt -s nullglob
for f in "$AUD"/*; do
  [ -f "$f" ] || continue
  b="$(basename "$f")"
  case "$b" in
    WATCH.log|watch_noop.log|README.md) continue ;;
  esac

  # Bỏ qua checkpoint Jupyter (bản sao cũ, dễ gây nhiễu).
  case "$b" in
    *-checkpoint.md|*.ipynb) continue ;;
  esac

  fp="$(stat -c '%Y %s' "$f" 2>/dev/null) $(sha256sum "$f" 2>/dev/null | cut -d' ' -f1)"
  key="$b|$fp"
  if ! grep -qFx "$key" "$SEEN"; then
    echo "$(date '+%Y-%m-%d %H:%M:%S') NEW/CHANGED AUDIT: $b" >> "$LOG"
    {
      echo ""
      echo "## $(date '+%Y-%m-%d %H:%M:%S') — verdict: $b"
      echo "=== NOI DUNG ($b) ==="
      cat "$f"
      echo ""
    } >> "$OUT"
    echo "$key" >> "$SEEN"
  fi
done
