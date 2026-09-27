#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ -z "${ROUTER_URL:-}" && -f .env ]]; then
  ROUTER_URL="$(python3 - <<'PY'
from pathlib import Path
base = ''
for raw in Path('.env').read_text(errors='ignore').splitlines():
    if raw.startswith('BASE_URL='):
        base = raw.split('=', 1)[1].strip().strip('"').strip("'")
        break
print(base or 'http://127.0.0.1:18080')
PY
)"
else
  ROUTER_URL="${ROUTER_URL:-http://127.0.0.1:18080}"
fi
OLLAYA_PORT="${OLLAYA_PORT:-11435}"
OLLAYA_HOST_BIND="${OLLAYA_HOST_BIND:-127.0.0.1:${OLLAYA_PORT}}"
OLLAYA_CONTAINER="${OLLAYA_CONTAINER:-brighto-ollaya-laya-smoke}"
OLLAYA_IMAGE="${OLLAYA_IMAGE:-ghcr.io/ollaya-dev/ollaya:latest}"
OLLAYA_MODEL="${OLLAYA_MODEL:-laya}"
BRIGHTO_PUBLIC_MODEL="${BRIGHTO_PUBLIC_MODEL:-ollaya-laya}"
BRIGHTO_GROUP_MODEL="${BRIGHTO_GROUP_MODEL:-ollaya-laya-group}"
BRIGHTO_BASE_URL="${BRIGHTO_BASE_URL:-http://127.0.0.1:${OLLAYA_PORT}/v1}"
AUTH_MODE="${AUTH_MODE:-none}"
OLLAYA_API_KEY="${OLLAYA_API_KEY:-}"
WAIT_AFTER_SAVE_SECONDS="${WAIT_AFTER_SAVE_SECONDS:-35}"
INSECURE_FLAG=()
case "$ROUTER_URL" in
  https://*) INSECURE_FLAG=(--insecure) ;;
esac

if [[ ! -f .env ]]; then
  echo "ERROR: .env not found. Run ./start.sh install first." >&2
  exit 1
fi

admin_key="$(python3 - <<'PY'
from pathlib import Path
for raw in Path('.env').read_text(errors='ignore').splitlines():
    if raw.startswith('ADMIN_MASTER_KEY='):
        print(raw.split('=',1)[1].strip().strip('"').strip("'"))
        raise SystemExit
PY
)"
client_key="$(python3 - <<'PY'
from pathlib import Path
for raw in Path('.env').read_text(errors='ignore').splitlines():
    if raw.startswith('BRIGHTO_ROUTER_API_KEY='):
        print(raw.split('=',1)[1].strip().strip('"').strip("'"))
        raise SystemExit
PY
)"
if [[ -z "$admin_key" || -z "$client_key" ]]; then
  echo "ERROR: ADMIN_MASTER_KEY or BRIGHTO_ROUTER_API_KEY missing in .env" >&2
  exit 1
fi

OLLAYA_DATA_DIR_EFFECTIVE="${OLLAYA_DATA_DIR:-$HOME/.ollaya-brighto-smoke}"
mkdir -p "$OLLAYA_DATA_DIR_EFFECTIVE"
if [[ ! -w "$OLLAYA_DATA_DIR_EFFECTIVE" ]]; then
  if command -v sudo >/dev/null 2>&1; then
    sudo -n chown -R "$(id -u):$(id -g)" "$OLLAYA_DATA_DIR_EFFECTIVE" 2>/dev/null || true
  else
    chown -R "$(id -u):$(id -g)" "$OLLAYA_DATA_DIR_EFFECTIVE" 2>/dev/null || true
  fi
fi
chmod u+rwx "$OLLAYA_DATA_DIR_EFFECTIVE" 2>/dev/null || true
if [[ ! -w "$OLLAYA_DATA_DIR_EFFECTIVE" ]]; then
  echo "ERROR: $OLLAYA_DATA_DIR_EFFECTIVE is not writable. Fix once with: sudo chown -R $(id -u):$(id -g) $OLLAYA_DATA_DIR_EFFECTIVE" >&2
  exit 1
fi

printf '\n==> Starting Ollaya container: %s\n' "$OLLAYA_CONTAINER"
docker rm -f "$OLLAYA_CONTAINER" >/dev/null 2>&1 || true
ollaya_env=(
  -e "OLLAYA_HOST=${OLLAYA_HOST_BIND}"
  -e "OLLAYA_MODELS=/home/ollaya/.ollaya/models"
)
if [[ -n "$OLLAYA_API_KEY" ]]; then
  ollaya_env+=( -e "OLLAYA_API_KEY=${OLLAYA_API_KEY}" )
  AUTH_MODE="bearer"
fi
docker run -d --name "$OLLAYA_CONTAINER" --network host \
  "${ollaya_env[@]}" \
  -v "$OLLAYA_DATA_DIR_EFFECTIVE:/home/ollaya/.ollaya" \
  "$OLLAYA_IMAGE" serve >/dev/null

printf '\n==> Waiting for Ollaya\n'
for _ in $(seq 1 90); do
  if curl -fsS --max-time 2 "http://${OLLAYA_HOST_BIND}/" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
curl -fsS --max-time 5 "http://${OLLAYA_HOST_BIND}/" >/dev/null

printf '\n==> Pulling Ollaya model: %s\n' "$OLLAYA_MODEL"
docker exec "$OLLAYA_CONTAINER" ollaya pull "$OLLAYA_MODEL"

printf '\n==> Direct Ollaya System One smoke\n'
OLLAYA_PORT="$OLLAYA_PORT" OLLAYA_MODEL="$OLLAYA_MODEL" OLLAYA_API_KEY="$OLLAYA_API_KEY" python3 - <<'PY'
import json, os, urllib.error, urllib.request
port = os.environ['OLLAYA_PORT']
model = os.environ['OLLAYA_MODEL']
key = os.environ.get('OLLAYA_API_KEY', '')
body = json.dumps({
  'model': model,
  'state': {'message': 'I was charged twice for one order.'},
  'questions': {'duplicate_charge': {'type': 'noul', 'instructions': 'Duplicate charge?'}}
}).encode()
headers = {'content-type': 'application/json'}
if key:
    headers['authorization'] = 'Bearer ' + key
req = urllib.request.Request(f'http://127.0.0.1:{port}/v1/systemone', data=body, headers=headers, method='POST')
with urllib.request.urlopen(req, timeout=90) as resp:
    data = json.loads(resp.read().decode())
answers = data.get('answers') or {}
if not answers:
    raise SystemExit('Ollaya returned no answers')
print('direct_ollaya_ok', sorted(answers))
PY

printf '\n==> Creating BrighTO System One route(s), key mode: %s\n' "$AUTH_MODE"
ADMIN_KEY="$admin_key" CLIENT_KEY="$client_key" ROUTER_URL="$ROUTER_URL" BRIGHTO_BASE_URL="$BRIGHTO_BASE_URL" AUTH_MODE="$AUTH_MODE" OLLAYA_API_KEY="$OLLAYA_API_KEY" BRIGHTO_PUBLIC_MODEL="$BRIGHTO_PUBLIC_MODEL" BRIGHTO_GROUP_MODEL="$BRIGHTO_GROUP_MODEL" OLLAYA_MODEL="$OLLAYA_MODEL" WAIT_AFTER_SAVE_SECONDS="$WAIT_AFTER_SAVE_SECONDS" python3 - <<'PY'
import json, os, ssl, time, urllib.error, urllib.request
base = os.environ['ROUTER_URL'].rstrip('/')
admin = os.environ['ADMIN_KEY']
client_key = os.environ['CLIENT_KEY']
backend_url = os.environ['BRIGHTO_BASE_URL']
auth_mode = os.environ['AUTH_MODE']
provider_key = os.environ.get('OLLAYA_API_KEY', '')
public_model = os.environ['BRIGHTO_PUBLIC_MODEL']
group_model = os.environ['BRIGHTO_GROUP_MODEL']
provider_model = os.environ['OLLAYA_MODEL']
ctx = ssl._create_unverified_context()
def http(path, method='GET', body=None, client=False):
    headers = {'Authorization': 'Bearer ' + client_key} if client else {'x-admin-key': admin}
    data = None
    if body is not None:
        headers['content-type'] = 'application/json'
        data = json.dumps(body).encode()
    req = urllib.request.Request(base + path, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, context=ctx, timeout=120) as resp:
            text = resp.read().decode()
            return resp.status, json.loads(text) if text else None, text
    except urllib.error.HTTPError as e:
        text = e.read().decode(errors='replace')
        return e.code, None, text

def admin_call(path, method='GET', body=None):
    st, js, txt = http(path, method, body)
    if st >= 400:
        raise SystemExit(f'{method} {path} failed {st}: {txt[:500]}')
    return js
# Test before save, using the same auth shape the route will use.
test_payload = {'base_url': backend_url, 'dialect': 'openai', 'auth_mode': auth_mode, 'provider_model_name': provider_model, 'protocol': 'systemone'}
if auth_mode != 'none':
    test_payload['provider_key'] = provider_key
result = admin_call('/admin/test-connection', 'POST', test_payload)
if not result.get('ok'):
    raise SystemExit(f'Test Connection failed: {result}')
print('brighto_test_connection_ok', result.get('detail'))
# Create two backends/routes so the smoke also proves Model Group round-robin.
stamp = str(int(time.time()))
route_names = []
backend_ids = []
for idx in (1, 2):
    b = admin_call('/admin/backends', 'POST', {'name': f'Ollaya Laya smoke {stamp}-{idx}', 'base_url': backend_url, 'api_key_ref': 'env:NONE', 'format': 'openai', 'weight': 1, 'max_inflight': 0, 'enabled': True})
    backend_ids.append(b['id'])
    rn = public_model if idx == 1 else f'{public_model}-b-{stamp}'
    payload = {'model_name': rn, 'backend_ids': [b['id']], 'chars_per_token': 4.0, 'first_byte_timeout': 90, 'provider_model_name': provider_model, 'enabled': True, 'auth_mode': auth_mode, 'protocol': 'systemone'}
    if auth_mode != 'none':
        payload['provider_key'] = provider_key
    st, _, txt = http('/admin/routes', 'POST', payload)
    if st == 409:
        rn = f'{public_model}-{stamp}-{idx}'
        payload['model_name'] = rn
        admin_call('/admin/routes', 'POST', payload)
    elif st >= 400:
        raise SystemExit(f'route create failed {st}: {txt[:500]}')
    route_names.append(rn)
endpoint_payload = []
for bid in backend_ids:
    ep = {'backend_id': bid, 'provider_model_name': provider_model, 'auth_mode': auth_mode, 'protocol': 'systemone', 'weight': 1, 'max_inflight': 0, 'enabled': True}
    if auth_mode != 'none':
        ep['provider_key'] = provider_key
    endpoint_payload.append(ep)
st, _, txt = http('/admin/routes', 'POST', {'model_name': group_model, 'backend_ids': backend_ids, 'chars_per_token': 4.0, 'first_byte_timeout': 90, 'provider_model_name': group_model, 'enabled': True, 'auth_mode': auth_mode, 'protocol': 'systemone', 'routing_policy': 'round_robin', 'endpoints': endpoint_payload})
if st == 409:
    group_model = f'{group_model}-{stamp}'
    admin_call('/admin/routes', 'POST', {'model_name': group_model, 'backend_ids': backend_ids, 'chars_per_token': 4.0, 'first_byte_timeout': 90, 'provider_model_name': group_model, 'enabled': True, 'auth_mode': auth_mode, 'protocol': 'systemone', 'routing_policy': 'round_robin', 'endpoints': endpoint_payload})
elif st >= 400:
    raise SystemExit(f'group create failed {st}: {txt[:500]}')
# Ensure the demo key can call these routes.
teams = admin_call('/admin/teams')
team_id = teams[0]['id']
smoke_key = admin_call('/admin/keys', 'POST', {'team_id': team_id, 'owner': f'systemone-smoke-{stamp}', 'allowed_models': route_names + [group_model], 'budget': None, 'rpm_limit': None, 'concurrency_limit': None, 'expires_at': None})['key']
body = {'model': '', 'state': {'message': 'I was charged twice for one order. Please refund it today.'}, 'questions': {'duplicate_charge': {'type': 'noul', 'instructions': 'Duplicate charge?'}, 'team': {'type': 'choice', 'instructions': 'Which team?', 'criteria': {'billing': 'payments and refunds', 'support': 'technical support'}}}}
client_key = smoke_key

def gateway(path, model):
    body['model'] = model
    st, js, txt = http(path, 'POST', body, client=True)
    answers = js.get('answers') if isinstance(js, dict) else None
    print('gateway_ok' if st < 400 and answers else 'gateway_fail', path, model, st, sorted(answers) if isinstance(answers, dict) else txt[:120])
    if st >= 400 or not isinstance(answers, dict) or not answers:
        raise SystemExit(f'gateway failed {path} {model}: {st} {txt[:500]}')

for model in route_names + [group_model]:
    gateway('/v1/systemone', model)
gateway('/v1/decisions', group_model)
wait_s = int(os.environ.get('WAIT_AFTER_SAVE_SECONDS', '35'))
print(f'waiting_health_interval {wait_s}s')
time.sleep(wait_s)
for model in route_names + [group_model]:
    gateway('/v1/systemone', model)
print(json.dumps({'routes': route_names, 'group': group_model}))
PY


printf '\nSystem One Ollaya/Laya smoke completed.\n'
