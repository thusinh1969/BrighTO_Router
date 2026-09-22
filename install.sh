#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${BRIGHTO_REPO:-https://github.com/thusinh1969/BrighTO_Router.git}"
REF="${BRIGHTO_REF:-main}"
INSTALL_DIR="${BRIGHTO_INSTALL_DIR:-$HOME/brighto-router}"
NO_START="${BRIGHTO_NO_START:-0}"

say() { printf '\n==> %s\n' "$*"; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
  cat <<'EOF'
BrighTO-Router one-line installer

Default:
  curl -fsSL https://raw.githubusercontent.com/thusinh1969/BrighTO_Router/main/install.sh | bash

Options through environment variables:
  BRIGHTO_INSTALL_DIR=$HOME/brighto-router   Install directory
  BRIGHTO_REPO=https://github.com/...         Git repository URL
  BRIGHTO_REF=main                            Git branch, tag, or commit
  BRIGHTO_NO_START=1                          Clone/update only; do not start Docker

Examples:
  BRIGHTO_INSTALL_DIR=/opt/brighto-router curl -fsSL URL | bash
  BRIGHTO_REF=main curl -fsSL URL | bash
EOF
}

case "${1:-}" in
  -h|--help) usage; exit 0 ;;
esac

need_runtime() {
  have git || fail "git is required. Install git first, then rerun this installer."
  have docker || fail "Docker is required. Install Docker Engine plus the Compose plugin first, then rerun this installer."
  docker compose version >/dev/null 2>&1 || fail "Docker Compose plugin is required. 'docker compose version' must work."
  docker info >/dev/null 2>&1 || fail "Docker is installed but not reachable by this user. Start Docker or add this user to the docker group, then rerun."
}

clone_or_update() {
  if [[ -d "$INSTALL_DIR/.git" ]]; then
    say "Updating BrighTO-Router in $INSTALL_DIR"
    git -C "$INSTALL_DIR" fetch --tags --prune origin
    git -C "$INSTALL_DIR" checkout "$REF"
    case "$REF" in
      main|master|preview-*|release/*)
        git -C "$INSTALL_DIR" pull --ff-only origin "$REF" || true
        ;;
    esac
  elif [[ -e "$INSTALL_DIR" ]]; then
    fail "$INSTALL_DIR exists but is not a Git checkout. Choose another BRIGHTO_INSTALL_DIR or move it aside."
  else
    say "Cloning BrighTO-Router into $INSTALL_DIR"
    mkdir -p "$(dirname "$INSTALL_DIR")"
    git clone "$REPO_URL" "$INSTALL_DIR"
    git -C "$INSTALL_DIR" fetch --tags --prune origin
    git -C "$INSTALL_DIR" checkout "$REF"
  fi
}

start_router() {
  cd "$INSTALL_DIR"
  [[ -x ./start.sh ]] || chmod +x ./start.sh
  say "Starting BrighTO-Router"
  ./start.sh install
}

need_runtime
clone_or_update

if [[ "$NO_START" == "1" ]]; then
  say "Installed source only"
  printf 'Next: cd %s && ./start.sh install\n' "$INSTALL_DIR"
  exit 0
fi

start_router
