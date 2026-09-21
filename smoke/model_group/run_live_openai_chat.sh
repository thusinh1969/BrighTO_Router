#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 smoke/model_group/live_openai_chat.py "$@"
