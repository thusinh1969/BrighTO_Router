#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

COMPOSE="${COMPOSE:-docker compose}"
COMPOSE_FILE_PATH="${COMPOSE_FILE_PATH:-$ROOT/docker-compose.yml}"
ENV_FILE="$ROOT/.env"
DEFAULT_URL="postgres://brighto_router:brighto_router_dev@127.0.0.1:55432/brighto_router"
DEFAULT_ADMIN_KEY="brightoIsGreat@2026"
DEFAULT_DEMO_CLIENT_KEY="sk-brighto-0123456789abcdef0123456789abcdef"
DEFAULT_LISTEN_ADDR="0.0.0.0:18080"
DEFAULT_ROUTER_IMAGE="thusinh1969/brighto_airouter:preview-2"
DEFAULT_PROVIDER_CATALOG='openai|OpenAI|https://api.openai.com|openai|OPENAI_API_KEY|1;anthropic|Anthropic|https://api.anthropic.com|anthropic|ANTHROPIC_API_KEY|1;gemini|Gemini|https://generativelanguage.googleapis.com/v1beta/openai|openai|GEMINI_API_KEY|0;deepseek|DeepSeek|https://api.deepseek.com|openai|DEEPSEEK_API_KEY|1;kimi|Kimi|https://api.moonshot.ai/v1|openai|KIMI_API_KEY|1;qwen|Qwen|https://dashscope-intl.aliyuncs.com/compatible-mode/v1|openai|QWEN_API_KEY|1;zai|Z.AI|https://api.z.ai/api/paas/v4|openai|ZAI_API_KEY|1;openrouter|OpenRouter|https://openrouter.ai/api/v1|openai|OPENROUTER_API_KEY|1;jina|Jina AI|https://api.jina.ai|openai|JINA_API_KEY|1;voyage|Voyage AI|https://api.voyageai.com|openai|VOYAGE_API_KEY|1;cohere|Cohere|https://api.cohere.com/v2|openai|COHERE_API_KEY|1;meta-muse|Meta Muse|https://api.meta.ai/v1|openai|META_MUSE_API_KEY|0;custom-llm|Custom LLM|http://127.0.0.1:8088/v1|openai|CUSTOM_LLM_API_KEY|1'
# Legacy defaults are kept only to upgrade old local .env files in place.
OLD_DEFAULT_LISTEN_ADDR="0.0.0.0:8080"
OLD_DEFAULT_URL="postgres://brighto_router:brighto_router_dev@127.0.0.1:5432/brighto_router"
OLD_DEFAULT_ADMIN_KEY="brighto-admin-dev"

say() { printf '\n==> %s\n' "$*"; }
fail() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

set_env_var() {
  local key="$1"
  local value="$2"
  KEY="$key" VALUE="$value" ENV_FILE="$ENV_FILE" python3 - <<'PY'
import os
from pathlib import Path
path = Path(os.environ["ENV_FILE"])
key = os.environ["KEY"]
value = os.environ["VALUE"]
lines = path.read_text().splitlines() if path.exists() else []
out = []
seen = False
for line in lines:
    if line.startswith(key + "="):
        out.append(f"{key}={value}")
        seen = True
    else:
        out.append(line)
if not seen:
    if out and out[-1].strip():
        out.append("")
    out.append(f"{key}={value}")
path.write_text("\n".join(out) + "\n")
PY
}

get_env_var() {
  local key="$1"
  local default_value="${2:-}"
  KEY="$key" DEFAULT_VALUE="$default_value" ENV_FILE="$ENV_FILE" python3 - <<'PY'
import os
from pathlib import Path
key = os.environ["KEY"]
default = os.environ.get("DEFAULT_VALUE", "")
path = Path(os.environ["ENV_FILE"])
if not path.exists():
    print(default)
    raise SystemExit
for raw in path.read_text().splitlines():
    line = raw.strip()
    if not line or line.startswith("#") or "=" not in line:
        continue
    name, value = line.split("=", 1)
    if name.strip() != key:
        continue
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "'\"":
        value = value[1:-1]
    print(value)
    raise SystemExit
print(default)
PY
}

ensure_env_defaults() {
  [[ -f "$ENV_FILE" ]] || return 0
  if grep -q "^ADMIN_MASTER_KEY=${OLD_DEFAULT_ADMIN_KEY}$" "$ENV_FILE"; then
    set_env_var ADMIN_MASTER_KEY "$DEFAULT_ADMIN_KEY"
  fi
  if grep -q "^DATABASE_URL=${OLD_DEFAULT_URL}$" "$ENV_FILE"; then
    set_env_var DATABASE_URL "$DEFAULT_URL"
  fi
  if grep -q '^DB_PORT=5432$' "$ENV_FILE" && grep -q "^DATABASE_URL=${DEFAULT_URL}$" "$ENV_FILE"; then
    set_env_var DB_PORT "55432"
  fi
  if grep -q "^LISTEN_ADDR=${OLD_DEFAULT_LISTEN_ADDR}$" "$ENV_FILE"; then
    set_env_var LISTEN_ADDR "$DEFAULT_LISTEN_ADDR"
  fi
  if ! grep -q "^PROVIDER_CATALOG=" "$ENV_FILE"; then
    set_env_var PROVIDER_CATALOG "\"$DEFAULT_PROVIDER_CATALOG\""
  fi
  if ! grep -q "^BRIGHTO_ROUTER_API_KEY=" "$ENV_FILE"; then
    set_env_var BRIGHTO_ROUTER_API_KEY "$DEFAULT_DEMO_CLIENT_KEY"
  fi
  if ! grep -q "^BRIGHTO_MODEL=" "$ENV_FILE"; then
    set_env_var BRIGHTO_MODEL ""
  fi
  if ! grep -q "^DATA_DIR=" "$ENV_FILE"; then
    set_env_var DATA_DIR "/var/lib/brighto-router"
  fi
  if ! grep -q "^BRIGHTO_ROUTER_IMAGE=" "$ENV_FILE"; then
    set_env_var BRIGHTO_ROUTER_IMAGE "$DEFAULT_ROUTER_IMAGE"
  elif grep -q "^BRIGHTO_ROUTER_IMAGE=thusinh1969/brighto_airouter:v1$" "$ENV_FILE"; then
    set_env_var BRIGHTO_ROUTER_IMAGE "$DEFAULT_ROUTER_IMAGE"
  fi
  local env_key
  for env_key in OPENAI_API_KEY ANTHROPIC_API_KEY GEMINI_API_KEY DEEPSEEK_API_KEY KIMI_API_KEY QWEN_API_KEY DASHSCOPE_API_KEY ZAI_API_KEY OPENROUTER_API_KEY JINA_API_KEY VOYAGE_API_KEY COHERE_API_KEY META_MUSE_API_KEY CUSTOM_LLM_API_KEY; do
    if ! grep -q "^${env_key}=" "$ENV_FILE"; then
      set_env_var "$env_key" ""
    fi
  done
}

ensure_env() {
  if [[ ! -f "$ENV_FILE" ]]; then
    [[ -f .env.example ]] || fail ".env is missing and .env.example was not found"
    cp .env.example .env
    chmod 600 .env
    cat >&2 <<MSG
Created .env from .env.example.
Default local admin key: ${DEFAULT_ADMIN_KEY}
Change ADMIN_MASTER_KEY before shared or production use.
MSG
  fi
  ensure_env_defaults
}

load_env() {
  ensure_env
  DATABASE_URL_EFFECTIVE="$(get_env_var DATABASE_URL "$DEFAULT_URL")"
  DB_HOST="$(get_env_var DB_HOST "127.0.0.1")"
  DB_PORT="$(get_env_var DB_PORT "55432")"
  DB_NAME="$(get_env_var DB_NAME "brighto_router")"
  DB_USER="$(get_env_var DB_USER "brighto_router")"
  DB_PASS="$(get_env_var DB_PASS "brighto_router_dev")"
  ADMIN_MASTER_KEY="$(get_env_var ADMIN_MASTER_KEY "")"
  LISTEN_ADDR_EFFECTIVE="$(get_env_var LISTEN_ADDR "$DEFAULT_LISTEN_ADDR")"
  HEALTH_HOST="$(get_env_var HEALTH_HOST "127.0.0.1")"
  HEALTH_PORT="${LISTEN_ADDR_EFFECTIVE##*:}"
  BASE_URL="$(get_env_var BASE_URL "http://${HEALTH_HOST}:${HEALTH_PORT}")"
}

compose() {
  [[ -f "$COMPOSE_FILE_PATH" ]] || fail "docker-compose.yml not found at $COMPOSE_FILE_PATH"
  # shellcheck disable=SC2086
  $COMPOSE -f "$COMPOSE_FILE_PATH" "$@"
}

ensure_runtime_dirs() {
  if [[ -e ssl && ! -d ssl ]]; then
    fail "ssl exists but is not a directory"
  fi
  mkdir -p ssl
  if [[ ! -w ssl ]]; then
    fail "ssl/ is not writable. Fix once with: sudo chown -R $(id -u):$(id -g) ssl"
  fi
}

uses_local_db() {
  [[ "${DATABASE_URL_EFFECTIVE:-$DEFAULT_URL}" == "$DEFAULT_URL" ]]
}

wait_postgres() {
  say "Waiting for Postgres"
  for _ in $(seq 1 60); do
    if compose exec -T postgres pg_isready -U "$DB_USER" -d "$DB_NAME" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  fail "Postgres did not become ready"
}

run_sql_file() {
  local file="$1"
  [[ -f "$file" ]] || fail "SQL file not found: $file"
  if command -v psql >/dev/null 2>&1; then
    PGPASSWORD="$DB_PASS" psql "$DATABASE_URL_EFFECTIVE" -v ON_ERROR_STOP=1 < "$file"
  elif uses_local_db; then
    compose exec -T postgres psql -v ON_ERROR_STOP=1 -U "$DB_USER" -d "$DB_NAME" < "$file"
  else
    fail "psql is required for external PostgreSQL installs"
  fi
}

run_migrations() {
  say "Running migrations"
  if command -v sqlx >/dev/null 2>&1; then
    DATABASE_URL="$DATABASE_URL_EFFECTIVE" sqlx migrate run
    return
  fi
  for migration in "$ROOT"/migrations/*.sql; do
    [[ -e "$migration" ]] || fail "no migrations found in $ROOT/migrations"
    run_sql_file "$migration"
  done
}

seed_defaults() {
  say "Seeding default team and provider templates"
  run_sql_file "$ROOT/scripts/seed_defaults.sql"
}

validate_runtime_env() {
  if [[ -z "${ADMIN_MASTER_KEY:-}" ]]; then
    fail "ADMIN_MASTER_KEY must be set in .env"
  fi
  validate_tls_files
}

validate_tls_files() {
  local cert_path key_path host_cert host_key
  cert_path="$(get_env_var TLS_CERT_PATH "")"
  key_path="$(get_env_var TLS_KEY_PATH "")"
  if [[ -z "$cert_path" && -z "$key_path" ]]; then
    return 0
  fi
  if [[ -z "$cert_path" || -z "$key_path" ]]; then
    fail "TLS_CERT_PATH and TLS_KEY_PATH must be set together, or both left empty"
  fi
  host_cert="$cert_path"
  host_key="$key_path"
  case "$cert_path" in /certs/*) host_cert="$ROOT/ssl/${cert_path#/certs/}" ;; esac
  case "$key_path" in /certs/*) host_key="$ROOT/ssl/${key_path#/certs/}" ;; esac
  if [[ ! -f "$host_cert" || ! -f "$host_key" ]]; then
    fail "TLS is enabled but cert/key files are missing. Expected: $host_cert and $host_key. Create them with: ./start.sh make-self-signed-cert HOST && ./start.sh tls --cert ssl/fullchain.pem --key ssl/privkey.pem --host HOST"
  fi
}

start_stack() {
  load_env
  validate_runtime_env
  if uses_local_db; then
    say "Starting local PostgreSQL"
    compose up -d postgres
    wait_postgres
  else
    say "Using external PostgreSQL from DATABASE_URL"
  fi
  run_migrations
  seed_defaults
  ensure_runtime_dirs
  say "Starting BrighTO-Router"
  compose up -d router
  wait_router
  say "Status"
  compose ps
  print_access_status
}

health() {
  local path="$1"
  curl -kfsS --max-time 2 "$BASE_URL$path" 2>/dev/null || true
}

copy_if_different() {
  local src="$1"
  local dest="$2"
  local src_abs=""
  local dest_abs=""
  src_abs="$(python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$src")"
  dest_abs="$(python3 -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$dest")"
  if [[ "$src_abs" != "$dest_abs" ]]; then
    cp "$src" "$dest"
  fi
}

is_https_enabled() {
  local cert_path key_path
  cert_path="$(get_env_var TLS_CERT_PATH "")"
  key_path="$(get_env_var TLS_KEY_PATH "")"
  [[ -n "$cert_path" && -n "$key_path" ]]
}

wait_router() {
  say "Waiting for BrighTO-Router"
  for _ in $(seq 1 60); do
    if [[ "$(health /readyz)" == "ready" ]]; then
      return 0
    fi
    sleep 1
  done
  fail "BrighTO-Router did not become ready at $BASE_URL"
}

print_access_status() {
  local scheme="HTTP"
  local port="${LISTEN_ADDR_EFFECTIVE##*:}"
  local server_hint=""
  server_hint="$(hostname -I | awk '{print $1}')"
  [[ -n "$server_hint" ]] || server_hint="<SERVER_IP>"
  if is_https_enabled; then
    scheme="HTTPS"
  fi
  printf '\nPortal: %s/\n' "$BASE_URL"
  printf '%s healthz: %s\n' "$scheme" "$(health /healthz)"
  printf '%s readyz:  %s\n' "$scheme" "$(health /readyz)"
  if is_https_enabled; then
    printf 'Local HTTPS test:  curl -k %s/readyz\n' "$BASE_URL"
    printf 'Remote HTTPS test: curl -k https://%s:%s/readyz\n' "$server_hint" "$port"
  else
    printf 'Local HTTP test:   curl %s/readyz\n' "$BASE_URL"
    printf 'Remote HTTP test:  curl http://%s:%s/readyz\n' "$server_hint" "$port"
    printf 'Enable HTTPS next: ./start.sh make-self-signed-cert HOST && ./start.sh tls --cert ssl/fullchain.pem --key ssl/privkey.pem --host HOST --port 18443\n'
  fi
}

provider_env_name() {
  case "$1" in
    openai) echo OPENAI_API_KEY ;;
    anthropic) echo ANTHROPIC_API_KEY ;;
    gemini) echo GEMINI_API_KEY ;;
    deepseek) echo DEEPSEEK_API_KEY ;;
    kimi) echo KIMI_API_KEY ;;
    qwen) echo QWEN_API_KEY ;;
    dashscope) echo DASHSCOPE_API_KEY ;;
    zai|z.ai) echo ZAI_API_KEY ;;
    jina) echo JINA_API_KEY ;;
    voyage) echo VOYAGE_API_KEY ;;
    cohere) echo COHERE_API_KEY ;;
    openrouter) echo OPENROUTER_API_KEY ;;
    meta|meta-muse|muse) echo META_MUSE_API_KEY ;;
    custom|custom-llm) echo CUSTOM_LLM_API_KEY ;;
    *) fail "unknown provider '$1'. Known: openai anthropic gemini deepseek kimi qwen dashscope zai openrouter jina voyage cohere meta-muse custom-llm" ;;
  esac
}

cmd_install() {
  local use_k8s=0
  local replicas=1
  local namespace=brighto-router
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --database-url)
        [[ $# -ge 2 ]] || fail "--database-url requires a value"
        ensure_env
        set_env_var DATABASE_URL "$2"
        shift 2
        ;;
      --admin-key)
        [[ $# -ge 2 ]] || fail "--admin-key requires a value"
        ensure_env
        set_env_var ADMIN_MASTER_KEY "$2"
        shift 2
        ;;
      --k8s)
        use_k8s=1
        shift
        ;;
      --replicas|--pods)
        [[ $# -ge 2 ]] || fail "$1 requires a number"
        replicas="$2"
        shift 2
        ;;
      --namespace)
        [[ $# -ge 2 ]] || fail "--namespace requires a value"
        namespace="$2"
        shift 2
        ;;
      *) fail "unknown install option: $1" ;;
    esac
  done
  if [[ "$use_k8s" == "1" ]]; then
    cmd_k8s --replicas "$replicas" --namespace "$namespace"
  else
    start_stack
  fi
}

write_k8s_env_file() {
  local target="$1"
  cp "$ENV_FILE" "$target"
  local k8s_db_url="${DATABASE_URL_EFFECTIVE}"
  if uses_local_db; then
    k8s_db_url="postgres://${DB_USER}:${DB_PASS}@brighto-router-postgres:5432/${DB_NAME}"
  fi
  ENV_FILE_TARGET="$target" K8S_DB_URL="$k8s_db_url" python3 - <<'PY2'
import os
from pathlib import Path
path = Path(os.environ["ENV_FILE_TARGET"])
updates = {
    "DATABASE_URL": os.environ["K8S_DB_URL"],
    "LISTEN_ADDR": "0.0.0.0:8080",
}
lines = path.read_text().splitlines()
out = []
seen = set()
for line in lines:
    if "=" in line:
        key = line.split("=", 1)[0]
        if key in updates:
            out.append(f"{key}={updates[key]}")
            seen.add(key)
            continue
    out.append(line)
for key, value in updates.items():
    if key not in seen:
        out.append(f"{key}={value}")
path.write_text("\n".join(out) + "\n")
PY2
}

run_k8s_local_sql_file() {
  local namespace="$1"
  local file="$2"
  [[ -f "$file" ]] || fail "SQL file not found: $file"
  kubectl -n "$namespace" exec -i deploy/brighto-router-postgres -- \
    psql -v ON_ERROR_STOP=1 -U "$DB_USER" -d "$DB_NAME" < "$file"
}

run_k8s_migrations_and_seed() {
  local namespace="$1"
  if uses_local_db; then
    say "Waiting for Kubernetes PostgreSQL"
    kubectl -n "$namespace" wait --for=condition=available deployment/brighto-router-postgres --timeout=180s
    say "Running Kubernetes migrations"
    for migration in "$ROOT"/migrations/*.sql; do
      [[ -e "$migration" ]] || fail "no migrations found in $ROOT/migrations"
      run_k8s_local_sql_file "$namespace" "$migration"
    done
    say "Seeding Kubernetes defaults"
    run_k8s_local_sql_file "$namespace" "$ROOT/scripts/seed_defaults.sql"
  else
    say "Running migrations against external PostgreSQL"
    run_migrations
    seed_defaults
  fi
}

cmd_k8s() {
  local replicas=1
  local namespace=brighto-router
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --replicas|--pods)
        [[ $# -ge 2 ]] || fail "$1 requires a number"
        replicas="$2"
        shift 2
        ;;
      --namespace)
        [[ $# -ge 2 ]] || fail "--namespace requires a value"
        namespace="$2"
        shift 2
        ;;
      *) fail "unknown k8s option: $1" ;;
    esac
  done
  command -v kubectl >/dev/null 2>&1 || fail "kubectl is required for k8s install"
  load_env
  say "Applying Kubernetes manifests namespace=$namespace replicas=$replicas"
  kubectl create namespace "$namespace" --dry-run=client -o yaml | kubectl apply -f -
  if uses_local_db; then
    kubectl -n "$namespace" apply -f k8s/postgres.dev.yaml
  else
    say "Using external PostgreSQL from DATABASE_URL; skipping dev PostgreSQL manifest"
  fi
  run_k8s_migrations_and_seed "$namespace"
  tmp_env="$(mktemp)"
  trap 'rm -f "$tmp_env"' RETURN
  write_k8s_env_file "$tmp_env"
  kubectl -n "$namespace" create secret generic brighto-router-secret \
    --from-env-file="$tmp_env" --dry-run=client -o yaml | kubectl apply -f -
  kubectl -n "$namespace" apply -f k8s/configmap.example.yaml
  REPLICAS="$replicas" python3 - <<'PY' | kubectl -n "$namespace" apply -f -
import os
from pathlib import Path
text = Path("k8s/deployment.yaml").read_text()
text = text.replace("replicas: 1", f"replicas: {os.environ['REPLICAS']}", 1)
print(text)
PY
  kubectl -n "$namespace" apply -f k8s/service.yaml
}


# Generate a self-signed cert into ssl/ (git-ignored). Default host = first LAN IP.
cmd_make_self_signed_cert() {
  local host="${1:-}"
  if [[ -z "$host" ]]; then
    host="$(hostname -I | awk '{print $1}')"
    [[ -n "$host" ]] || fail "cannot detect host IP; pass an explicit host/IP argument"
  fi
  command -v openssl >/dev/null 2>&1 || fail "openssl is required"
  ensure_runtime_dirs
  say "Generating self-signed cert for $host into ssl/"
  local san=""
  if [[ "$host" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    san="IP:${host},IP:127.0.0.1,DNS:localhost,DNS:brighto-router"
  else
    san="DNS:${host},DNS:localhost,DNS:brighto-router,IP:127.0.0.1"
    local lan_ip=""
    lan_ip="$(hostname -I | awk '{print $1}')"
    if [[ -n "$lan_ip" && "$lan_ip" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      san="$san,IP:$lan_ip"
    fi
  fi
  openssl req -x509 -newkey rsa:4096 -sha256 -days 3650 -nodes \
    -keyout ssl/privkey.pem -out ssl/fullchain.pem \
    -subj "/CN=${host}" \
    -addext "subjectAltName=${san}"
  chmod 644 ssl/privkey.pem
  chmod 644 ssl/fullchain.pem
  say "Done. Enable with: ./start.sh tls --cert ssl/fullchain.pem --key ssl/privkey.pem --host $host --port 18443"
}

# Configure TLS: copy cert/key into ssl/, set .env (LISTEN_ADDR/BASE_URL/TLS_*), restart router.
cmd_tls() {
  local cert="" key="" host="" port="18443"
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --cert) [[ $# -ge 2 ]] || fail "--cert requires a value"; cert="$2"; shift 2 ;;
      --key)  [[ $# -ge 2 ]] || fail "--key requires a value";  key="$2";  shift 2 ;;
      --host) [[ $# -ge 2 ]] || fail "--host requires a value"; host="$2"; shift 2 ;;
      --port) [[ $# -ge 2 ]] || fail "--port requires a value"; port="$2"; shift 2 ;;
      *) fail "unknown tls option: $1" ;;
    esac
  done
  [[ -n "$cert" && -n "$key" ]] || fail "usage: ./start.sh tls --cert <cert> --key <key> --host <host-or-ip> [--port 18443]"
  [[ -f "$cert" && -f "$key" ]] || fail "cert or key file not found"
  if [[ -z "$host" ]]; then
    host="$(hostname -I | awk '{print $1}')"
    [[ -n "$host" ]] || fail "cannot detect host IP; pass --host"
  fi

  ensure_env
  ensure_runtime_dirs
  copy_if_different "$cert" ssl/fullchain.pem
  copy_if_different "$key" ssl/privkey.pem
  chmod 644 ssl/privkey.pem
  chmod 644 ssl/fullchain.pem

  set_env_var LISTEN_ADDR "0.0.0.0:$port"
  set_env_var BASE_URL "https://$host:$port"
  set_env_var TLS_CERT_PATH "/certs/fullchain.pem"
  set_env_var TLS_KEY_PATH "/certs/privkey.pem"

  load_env
  say "TLS enabled (LISTEN_ADDR=0.0.0.0:$port, BASE_URL=https://$host:$port). Recreating router."
  compose up -d --force-recreate router
  wait_router
  compose ps
  print_access_status
}

cmd="${1:-help}"
shift || true
case "$cmd" in
  install)
    cmd_install "$@"
    ;;
  start)
    start_stack
    ;;
  stop)
    if [[ ! -f "$ENV_FILE" ]]; then
      say ".env is missing; no configured Compose stack to stop"
      exit 0
    fi
    say "Stopping BrighTO-Router stack"
    compose down
    ;;
  restart)
    load_env
    validate_runtime_env
    say "Restarting BrighTO-Router stack"
    if uses_local_db; then
      compose up -d postgres
      wait_postgres
    fi
    run_migrations
    seed_defaults
    ensure_runtime_dirs
    compose up -d --force-recreate router
    wait_router
    compose ps
    print_access_status
    ;;
  status)
    if [[ ! -f "$ENV_FILE" ]]; then
      say ".env is missing; run ./start.sh install to create it from .env.example"
      exit 0
    fi
    load_env
    compose ps
    print_access_status
    ;;
  logs)
    load_env
    compose logs -f router
    ;;
  migrate)
    load_env
    if uses_local_db; then
      compose up -d postgres
      wait_postgres
    fi
    run_migrations
    ;;
  seed)
    load_env
    if uses_local_db; then
      compose up -d postgres
      wait_postgres
    fi
    run_migrations
    seed_defaults
    ;;
  set-key)
    [[ $# -ge 2 ]] || fail "usage: ./start.sh set-key <provider> <api-key>"
    ensure_env
    env_name="$(provider_env_name "$1")"
    set_env_var "$env_name" "$2"
    say "Updated $env_name in .env"
    if compose ps -q router >/dev/null 2>&1 && [[ -n "$(compose ps -q router 2>/dev/null || true)" ]]; then
      say "Recreating router so Docker receives the updated environment"
      compose up -d --force-recreate router
    fi
    ;;
  k8s)
    cmd_k8s "$@"
    ;;
  smoke)
    load_env
    say "Running short benchmark smoke"
    make bench-gate-smoke
    ;;
  gate)
    load_env
    say "Running release gate"
    make gate
    ;;
  build)
    say "Building release binary"
    cargo build --release --locked
    ;;
  tls)
    cmd_tls "$@"
    ;;
  make-self-signed-cert)
    cmd_make_self_signed_cert "$@"
    ;;
  help|-h|--help)
    cat <<'USAGE'
BrighTO-Router helper

First-time install:
  ./start.sh install                         Local Docker PostgreSQL + migrations + provider templates + preview-2 router
  ./start.sh install --database-url URL      Use an existing PostgreSQL database
  ./start.sh install --k8s --replicas 2      Install to Kubernetes with two router pods

Daily operation:
  ./start.sh start       Start local/external DB flow and router
  ./start.sh stop        Stop Docker Compose stack
  ./start.sh restart     Run migrations, seed templates, recreate router
  ./start.sh status      Show containers plus /healthz and /readyz
  ./start.sh logs        Follow router logs
  ./start.sh migrate     Run SQL migrations
  ./start.sh seed        Seed Default Team and provider templates
  ./start.sh set-key openai sk-...           Save provider API key to .env and recreate router
  ./start.sh smoke       Run a short non-release benchmark smoke
  ./start.sh gate        Run the release gate from Makefile
  ./start.sh build       Build target/release/brighto-router
  ./start.sh make-self-signed-cert [HOST]    Generate a self-signed cert into ssl/
  ./start.sh tls --cert ssl/fullchain.pem --key ssl/privkey.pem --host HOST --port 18443   Enable HTTPS and recreate router

Default local admin key:
  brightoIsGreat@2026

Provider names for set-key:
  openai anthropic gemini deepseek kimi qwen dashscope zai openrouter jina voyage cohere meta-muse custom-llm
USAGE
    ;;
  *)
    fail "unknown command: $cmd (try ./start.sh help)"
    ;;
esac
