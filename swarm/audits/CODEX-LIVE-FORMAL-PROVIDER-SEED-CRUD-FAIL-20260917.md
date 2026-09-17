# CODEX AUDIT — Live Formal Provider Seed + CRUD Probe

Date: 2026-09-17 13:15 ICT  
Live target: `https://127.0.0.1:18443`  
Docker image rebuilt/recreated before seed: `thusinh1969/brighto_airouter:v1`  
Seed script used: `swarm/scripts/live_formal_provider_setup.py`  
CRUD probe artifact: `swarm/out/playwright/20260917T061445-portal-crud-probe/summary.json`

## Live data was cleaned as requested

Current providers are now exactly formal providers plus one custom/local provider:

```text
1  openai              enabled
2  anthropic           enabled
3  gemini              disabled, no route
4  deepseek            enabled
5  kimi                enabled
6  qwen                enabled, non-token-plan base URL
7  zai                 enabled
8  openrouter          disabled, no key found
9  meta-muse           disabled, malformed/unclear config
10 custom-local-llama  enabled, http://127.0.0.1:8088/v1
```

Removed providers/routes:

- `Local llama.cpp` duplicate provider.
- `DeepSeek V4 Pro` duplicate provider.
- stale `pw-*`, `crud-*`, `verify-*` test providers/routes.
- old ad-hoc routes `deepseek-v4-pro`, `qwen-local`, `qwen3.8-flash-next`.

Current test routes:

```text
test-openai        -> openai      / gpt-5.6             / openai_chat        / bearer
test-anthropic     -> anthropic   / claude-opus-5       / anthropic_messages / anthropic
test-deepseek      -> deepseek    / deepseek-v4-pro     / openai_chat        / bearer
test-kimi          -> kimi        / kimi-k3             / openai_chat        / bearer
test-qwen          -> qwen        / qwen-3.8-max        / openai_chat        / bearer
test-zai           -> zai         / glm-5.2             / openai_chat        / bearer
test-custom-local  -> custom      / qwen3.8-flash-next  / local_openai_chat  / none
```

`DATA_DIR=/var/lib/brighto-router` was added to local `.env` so route-level key files are written into the Docker volume instead of the old non-mounted `/var/lib/llm-router` default.

## Route smoke result

First seed smoke:

```text
PASS test-deepseek
PASS test-kimi
PASS test-custom-local
FAIL test-openai      400: gpt-5.6 needs max_completion_tokens, not max_tokens
FAIL test-anthropic   400: Anthropic route must be called via /v1/messages, not /v1/chat/completions
FAIL test-qwen        503 no healthy backend available
FAIL test-zai         503 no healthy backend available
```

Second protocol-aware smoke, after a few seconds:

```text
FAIL test-openai      503 no healthy backend available
FAIL test-anthropic   503 no healthy backend available
FAIL test-deepseek    503 no healthy backend available
FAIL test-kimi        503 no healthy backend available
FAIL test-qwen        503 no healthy backend available
FAIL test-zai         503 no healthy backend available
PASS test-custom-local
```

## Root cause: cloud health/circuit logic is wrong for authenticated providers

The router health loop checks each backend with unauthenticated:

- `GET {base}/health`
- then `GET {base}/v1/models`

For cloud providers those endpoints usually require auth and return 401/403. The router then treats cloud backend as unhealthy / circuit-open, so enabled routes later return:

```text
503 no healthy backend available
```

This explains why DeepSeek and Kimi pass immediately after route creation but later become 503.

Required fix:

- For cloud/authenticated providers, do not use unauthenticated `/v1/models` health as a circuit-breaker signal.
- Minimal v1 fix: skip background health check for `auth_mode != none` / cloud routes, and rely on request success/failure to update circuit.
- Better simple fix: health check route-aware with provider key, but avoid overengineering. Do not add Redis.
- A 401/403 from unauthenticated health must not poison the backend circuit for route traffic that has a valid route-level key.

Acceptance:

- After waiting at least 30 seconds, cloud routes still smoke through router instead of flipping to 503.
- `test-deepseek` and `test-kimi` must remain PASS after health loop runs.

## CRUD probe result

Playwright CRUD probe on live Portal:

```text
PASS provider list is clean formal + one custom
PASS in-use custom-local provider delete disabled
PASS create provider via UI ok
PASS edit provider via UI patches same id
FAIL delete provider after edit is not clickable
FAIL delete provider via UI left backend
PASS create model route via UI ok
PASS edit model route via UI ok
FAIL delete model route via UI left route
PASS re-create model route via UI ok
```

Provider Delete failure evidence:

```text
locator.click: Timeout 5000ms exceeded.
```

Focused reproduction also showed normal click fails after row edit even after 5 seconds, with no confirm dialog:

```text
before delete wait 500 class=flash
click failed after wait 500
before delete wait 1800 class=
click failed after wait 1800
before delete wait 5000 class=
click failed after wait 5000
exists=true dialogs=[]
```

Required Provider CRUD fix:

- After Provider edit/save and rerender, action buttons must be normal clickable elements.
- Check CSS/table/sticky action cell/row flash animation/hit testing. Do not fix tests with forced clicks.
- Manual expected flow: Add provider -> Edit provider -> Save -> Delete -> confirm appears -> backend deleted -> table refreshes.

Required Model Route CRUD fix:

- Route Delete button must delete the selected route via UI and refresh the table.
- Test exact flow: Create route -> Edit route -> Delete edited route -> API confirms it is gone -> Create same route again.

## Do not claim green until

Run all of these on rebuilt/recreated Docker:

```bash
bash swarm/scripts/portal_logic_acceptance.sh
python3 swarm/scripts/live_formal_provider_setup.py
```

Then run a Playwright CRUD probe covering:

1. Provider create/edit/delete/recreate.
2. Provider in-use delete disabled.
3. Route create/edit/delete/recreate.
4. Route smoke after health loop has run for at least 30 seconds.

Final acceptance must not use `BRIGHTO_SKIP_ROUTE_SMOKE=1`.
