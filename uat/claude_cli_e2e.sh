#!/usr/bin/env bash
# Real Claude CLI / Claude Code end-to-end check against the single UAT model:
# local Qwen 3.8 exposed as both OpenAI Chat and Anthropic Messages.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UAT_DIR="$ROOT/uat"
ENV_FILE="$UAT_DIR/.env.uat"

fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }
[[ -f "$ENV_FILE" ]] || fail "missing $ENV_FILE; run ./uat/start.sh up first"
set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
set +a
: "${UAT_CLIENT_KEY:?UAT_CLIENT_KEY missing}"
command -v claude >/dev/null 2>&1 || fail "claude CLI is not installed; Claude CLI e2e is UNTESTED"
command -v timeout >/dev/null 2>&1 || fail "timeout command is required"

UAT_BASE_URL="${UAT_BASE_URL:-https://127.0.0.1:18081}"
UAT_MODEL="${UAT_E2E_MODEL:-${UAT_QWEN_MODEL:-qwen3.8-flash-next}}"
WORK="$UAT_DIR/work"
CONFIG="$UAT_DIR/claude-config"
mkdir -p "$WORK" "$CONFIG"
printf 'claude-bridge-uat\n' > "$WORK/uat.txt"
if [[ -n "${UAT_CA_CERT:-}" && -f "$UAT_CA_CERT" ]]; then
  export NODE_EXTRA_CA_CERTS="$UAT_CA_CERT"
fi

run_claude() {
  local prompt="$1"
  (
    cd "$WORK"
    env -u ANTHROPIC_AUTH_TOKEN \
      ANTHROPIC_BASE_URL="$UAT_BASE_URL" \
      ANTHROPIC_API_KEY="$UAT_CLIENT_KEY" \
      ANTHROPIC_MODEL="$UAT_MODEL" \
      ANTHROPIC_DEFAULT_HAIKU_MODEL="$UAT_MODEL" \
      ANTHROPIC_DEFAULT_SONNET_MODEL="$UAT_MODEL" \
      ANTHROPIC_DEFAULT_OPUS_MODEL="$UAT_MODEL" \
      ANTHROPIC_DEFAULT_FABLE_MODEL="$UAT_MODEL" \
      CLAUDE_CONFIG_DIR="$CONFIG" \
      CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 \
      DISABLE_TELEMETRY=1 \
      DISABLE_ERROR_REPORTING=1 \
      timeout 180 claude -p "$prompt" --output-format text \
        --dangerously-skip-permissions --max-turns 6 </dev/null
  )
}

printf '==> Claude CLI version: %s\n' "$(claude --version 2>/dev/null | head -1)"
printf '==> Base URL: %s, model: %s\n' "$UAT_BASE_URL" "$UAT_MODEL"

text_output="$(run_claude "Reply with exactly QWEN_CLAUDE_OK. Do not use tools.")"
printf '%s\n' "$text_output"
printf '%s' "$text_output" | grep -q "QWEN_CLAUDE_OK" \
  || fail "Claude CLI text response did not contain QWEN_CLAUDE_OK"
printf 'PASS Claude CLI text request through the Qwen Anthropic bridge\n'

tool_output="$(run_claude "Read the file uat.txt in the current directory with the Read tool, then reply with its content.")"
printf '%s\n' "$tool_output"
printf '%s' "$tool_output" | grep -q "claude-bridge-uat" \
  || fail "Claude CLI tool response did not contain claude-bridge-uat"
printf 'PASS Claude CLI tool_use/tool_result round trip through the Qwen Anthropic bridge\n'
printf 'CLAUDE CLI + QWEN 3.8 UAT PASSED\n'
