#!/usr/bin/env python3
import argparse
import os
import pathlib
import time

import requests
import urllib3
urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)

REPO = pathlib.Path(__file__).resolve().parent.parent
BASE = 'https://127.0.0.1:18443'
DEEPSEEK_BASE = 'https://api.deepseek.com'
LOCAL_BASE = 'http://127.0.0.1:8088/v1'
DS_MODEL = 'deepseek-v4-pro'
LOCAL_MODEL = 'qwen3.8-flash-next'
ADMIN = ''
DS_KEY = ''
NAMES = {
    'ds_backend': 'live-lb-deepseek-backend',
    'local_backend': 'live-lb-qwen38-backend',
    'ds_route': 'live-lb-deepseek-v4-pro',
    'local_route': 'live-lb-qwen38',
    'rr_group': 'live-lb-rr',
    'weighted_group': 'live-lb-weighted',
    'team': 'Live LB Test',
    'owner': 'live-lb-test@local',
}

def read_env():
    vals = dict(os.environ)
    p = REPO / '.env'
    if p.exists():
        for raw in p.read_text(errors='ignore').splitlines():
            line = raw.strip()
            if not line or line.startswith('#') or '=' not in line: continue
            k,v = line.split('=',1)
            v = v.strip()
            if len(v)>=2 and v[0]==v[-1] and v[0] in '"\'': v = v[1:-1]
            vals.setdefault(k.strip(), v)
    return vals

def parse_args():
    ap = argparse.ArgumentParser(description='Real DeepSeek + local Qwen Model Group load-balancing smoke test.')
    ap.add_argument('--router', default=os.environ.get('BRIGHTO_ROUTER_URL', BASE), help='Router base URL')
    ap.add_argument('--local-base', default=os.environ.get('BRIGHTO_LOCAL_LLM_BASE', LOCAL_BASE), help='local llama.cpp OpenAI-compatible base URL')
    ap.add_argument('--deepseek-base', default=os.environ.get('BRIGHTO_DEEPSEEK_BASE', DEEPSEEK_BASE), help='DeepSeek OpenAI-compatible base URL')
    ap.add_argument('--deepseek-model', default=os.environ.get('BRIGHTO_DEEPSEEK_MODEL', DS_MODEL), help='DeepSeek provider model name')
    ap.add_argument('--local-model', default=os.environ.get('BRIGHTO_LOCAL_MODEL', LOCAL_MODEL), help='local provider model name')
    return ap.parse_args()

def configure():
    global BASE, LOCAL_BASE, DEEPSEEK_BASE, DS_MODEL, LOCAL_MODEL, ADMIN, DS_KEY, AH
    args = parse_args()
    BASE = args.router.rstrip('/')
    LOCAL_BASE = args.local_base.rstrip('/')
    DEEPSEEK_BASE = args.deepseek_base.rstrip('/')
    DS_MODEL = args.deepseek_model
    LOCAL_MODEL = args.local_model
    env = read_env()
    ADMIN = env.get('ADMIN_MASTER_KEY','').strip()
    DS_KEY = (env.get('DEEPSEEK_API_KEY') or env.get('DEEPSEEK_KEY') or '').strip()
    if not ADMIN: raise SystemExit('ADMIN_MASTER_KEY missing')
    if not DS_KEY: raise SystemExit('DEEPSEEK_API_KEY missing')
    AH = {'x-admin-key': ADMIN, 'content-type':'application/json'}

AH = {}

def admin(method, path, body=None, ok=(200,204)):
    r = requests.request(method, BASE+path, headers=AH, json=body, verify=False, timeout=120)
    if r.status_code not in ok:
        raise RuntimeError(f'{method} {path} -> {r.status_code}: {r.text[:500]}')
    return None if not r.text else r.json()

def route_by_name(name):
    for r in admin('GET','/admin/routes'):
        if r.get('model_name') == name: return r
    return None

def backend_by_name(name):
    for b in admin('GET','/admin/backends'):
        if b.get('name') == name: return b
    return None

def ensure_backend(name, base_url, fmt='openai'):
    b = backend_by_name(name)
    payload = {'name': name, 'base_url': base_url, 'api_key_ref':'env:NONE', 'format': fmt, 'weight':1, 'max_inflight':0, 'enabled': True}
    if b:
        admin('PATCH', f'/admin/backends/{b["id"]}', payload)
        b = backend_by_name(name)
    else:
        b = admin('POST','/admin/backends', payload)
    return b

def upsert_route(name, payload):
    old = route_by_name(name)
    if old:
        return admin('PATCH', f'/admin/routes/{name}', payload)
    return admin('POST','/admin/routes', payload)

def ensure_team_key(models):
    teams = admin('GET','/admin/teams')
    team = next((t for t in teams if t.get('name') == NAMES['team']), None)
    if not team:
        team = admin('POST','/admin/teams', {'name':NAMES['team'], 'budget':None, 'enabled':True})
    key = admin('POST','/admin/keys', {'team_id':team['id'], 'owner':NAMES['owner'], 'allowed_models':models, 'budget':None, 'rpm_limit':60, 'concurrency_limit':4, 'expires_at':None})
    return key['key']

def preview_models(base_url, auth_mode, key=None):
    body = {'base_url':base_url, 'protocol':'openai', 'auth_mode':auth_mode}
    if key: body['provider_key'] = key
    r = admin('POST','/admin/routes/preview-models', body)
    return r.get('models', [])

def call_model(client_key, model, label):
    h = {'authorization':'Bearer '+client_key, 'content-type':'application/json'}
    body = {'model':model, 'messages':[{'role':'user','content':'Reply with one short word: OK'}], 'max_tokens':4, 'temperature':0, 'stream':False}
    t0 = time.time()
    r = requests.post(BASE+'/v1/chat/completions', headers=h, json=body, verify=False, timeout=420)
    dt = time.time()-t0
    if r.status_code != 200:
        raise RuntimeError(f'{label} {model} -> {r.status_code}: {r.text[:500]}')
    data = r.json()
    if 'choices' not in data:
        raise RuntimeError(f'{label} {model} missing choices: {str(data)[:300]}')
    print(f'PASS call {label}: {model} ({dt:.2f}s)')
    return data

def usage_counts(model, backend_ids, from_ts):
    rows = admin('GET', f'/admin/usage?model={model}&from={from_ts}')
    counts = {int(b):0 for b in backend_ids}
    statuses = []
    for row in rows:
        if row.get('model') != model: continue
        bid = int(row.get('backend_id'))
        if bid in counts: counts[bid] += 1
        statuses.append(row.get('status'))
    return counts, statuses

def wait_counts(model, backend_ids, from_ts, expected_total, timeout=30):
    deadline = time.time()+timeout
    last = None
    while time.time() < deadline:
        counts, statuses = usage_counts(model, backend_ids, from_ts)
        total = sum(counts.values())
        last = (counts, statuses)
        if total >= expected_total:
            return counts, statuses
        time.sleep(1)
    raise RuntimeError(f'usage ledger timeout for {model}; last={last}')

def main():
    configure()
    print('Preparing real DeepSeek + local Qwen routes and groups (keys hidden).')
    print(f'Router={BASE}; DeepSeek model={DS_MODEL}; local model={LOCAL_MODEL}')
    # Provider liveness/model checks.
    local_models = preview_models(LOCAL_BASE, 'none')
    if LOCAL_MODEL not in local_models:
        raise RuntimeError(f'local model {LOCAL_MODEL} not in preview list: {local_models[:10]}')
    print(f'PASS local preview includes {LOCAL_MODEL}')
    ds_models = preview_models(DEEPSEEK_BASE, 'bearer', DS_KEY)
    if DS_MODEL not in ds_models:
        raise RuntimeError(f'DeepSeek model {DS_MODEL} not in preview list; first={ds_models[:10]}')
    print(f'PASS DeepSeek preview includes {DS_MODEL}')

    dsb = ensure_backend(NAMES['ds_backend'], DEEPSEEK_BASE)
    lb = ensure_backend(NAMES['local_backend'], LOCAL_BASE)
    did, lid = int(dsb['id']), int(lb['id'])
    print(f'Backends: DeepSeek={did}, LocalQwen={lid}')

    ds_route = upsert_route(NAMES['ds_route'], {
        'model_name':NAMES['ds_route'], 'backend_ids':[did], 'provider_model_name':DS_MODEL,
        'provider_key':DS_KEY, 'auth_mode':'bearer', 'protocol':'openai_chat', 'enabled':True, 'first_byte_timeout':300,
    })
    local_route = upsert_route(NAMES['local_route'], {
        'model_name':NAMES['local_route'], 'backend_ids':[lid], 'provider_model_name':LOCAL_MODEL,
        'auth_mode':'none', 'protocol':'local_openai_chat', 'enabled':True, 'first_byte_timeout':300,
    })
    routes = admin('GET','/admin/routes')
    ds_route = next(r for r in routes if r['model_name']==NAMES['ds_route'])
    local_route = next(r for r in routes if r['model_name']==NAMES['local_route'])
    ds_key_ref = ds_route.get('provider_key_ref')
    if not ds_key_ref: raise RuntimeError('DeepSeek route saved without provider_key_ref')
    print('PASS source routes saved; DeepSeek credential stored as key reference')

    endpoint_ds = {'backend_id':did, 'provider_model_name':DS_MODEL, 'provider_key_ref':ds_key_ref, 'auth_mode':'bearer', 'protocol':'openai_chat', 'weight':1, 'max_inflight':0, 'enabled':True}
    endpoint_local = {'backend_id':lid, 'provider_model_name':LOCAL_MODEL, 'auth_mode':'none', 'protocol':'local_openai_chat', 'weight':1, 'max_inflight':0, 'enabled':True}
    rr = upsert_route(NAMES['rr_group'], {
        'model_name':NAMES['rr_group'], 'backend_ids':[did,lid], 'provider_model_name':NAMES['rr_group'],
        'enabled':True, 'auth_mode':'bearer', 'protocol':'openai_chat', 'routing_policy':'round_robin',
        'first_byte_timeout':300, 'endpoints':[endpoint_ds, endpoint_local],
    })
    endpoint_ds_w = dict(endpoint_ds); endpoint_ds_w['weight'] = 3
    endpoint_local_w = dict(endpoint_local); endpoint_local_w['weight'] = 1
    wg = upsert_route(NAMES['weighted_group'], {
        'model_name':NAMES['weighted_group'], 'backend_ids':[did,lid], 'provider_model_name':NAMES['weighted_group'],
        'enabled':True, 'auth_mode':'bearer', 'protocol':'openai_chat', 'routing_policy':'weighted_round_robin',
        'first_byte_timeout':300, 'endpoints':[endpoint_ds_w, endpoint_local_w],
    })
    print('PASS Model Groups saved: round_robin and weighted_round_robin 3:1')

    client_key = ensure_team_key([NAMES['ds_route'], NAMES['local_route'], NAMES['rr_group'], NAMES['weighted_group']])
    call_model(client_key, NAMES['ds_route'], 'source DeepSeek')
    call_model(client_key, NAMES['local_route'], 'source Local Qwen')

    rr_from = int(time.time()) - 1
    for i in range(4): call_model(client_key, NAMES['rr_group'], f'round-robin #{i+1}')
    rr_counts, rr_statuses = wait_counts(NAMES['rr_group'], [did,lid], rr_from, 4)
    print(f'Round-robin usage counts: DeepSeek={rr_counts[did]}, LocalQwen={rr_counts[lid]}, statuses={sorted(set(rr_statuses))}')
    if rr_counts[did] != 2 or rr_counts[lid] != 2:
        raise RuntimeError(f'round-robin expected 2/2, got {rr_counts}')

    wg_from = int(time.time()) - 1
    for i in range(4): call_model(client_key, NAMES['weighted_group'], f'weighted #{i+1}')
    wg_counts, wg_statuses = wait_counts(NAMES['weighted_group'], [did,lid], wg_from, 4)
    print(f'Weighted usage counts: DeepSeek={wg_counts[did]}, LocalQwen={wg_counts[lid]}, statuses={sorted(set(wg_statuses))}')
    if wg_counts[did] != 3 or wg_counts[lid] != 1:
        raise RuntimeError(f'weighted 3:1 expected 3/1, got {wg_counts}')

    print('RESULT PASS real DeepSeek+Qwen round-robin and weighted Model Group test')

if __name__ == '__main__':
    main()
