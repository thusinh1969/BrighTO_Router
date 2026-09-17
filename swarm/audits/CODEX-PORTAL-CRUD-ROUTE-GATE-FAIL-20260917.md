# CODEX AUDIT — Portal CRUD + Route Viability Gate FAIL

Date: 2026-09-17 13:08 ICT  
Live target: `https://127.0.0.1:18443`  
Image rebuilt before test: `thusinh1969/brighto_airouter:v1`  
Acceptance command: `bash swarm/scripts/portal_logic_acceptance.sh`  
Artifact: `swarm/out/playwright/20260917-130813-portal-logic-acceptance/summary.json`

This is not green. Do not mark the Portal done until the command above passes on live Docker after rebuild/recreate.

## Current passes

- Admin login over HTTPS works.
- Compact formatter works: `1K`, `50K`, `1M`; compact density is live.
- Provider page no longer shows provider-row cloud `Load models`.
- Provider copy now correctly says API key belongs in the Route wizard.
- Provider `Edit` is visible at 1280px and 1024px.
- Provider modal has `Provider Type`.
- Provider create through UI persists exactly one backend.
- Provider edit updates the same backend id, not a duplicate row.
- Route provider dropdown has no duplicate display names.
- Blank-key cloud `Load models` now blocks locally with `Enter API key first`; it no longer leaks provider 401/502.
- Route fields persist: provider model, protocol, auth mode, context window, max output tokens, input/output price.
- API key reveal and edit work after async reveal finishes.
- User login hides admin menus and lands on Dashboard.
- Local llama.cpp routes smoke pass: `qwen-local`, `qwen3.8-flash-next`.

## Blocking failures to fix now

### 1) Provider used by a route still has an enabled Delete button

Evidence from gate:

```json
{
  "provider": "DeepSeek V4 Pro",
  "id": 19,
  "state": {
    "disabled": false,
    "ariaDisabled": null,
    "title": null,
    "className": "btn sm danger"
  }
}
```

Required fix:

- If a backend/provider is referenced by any `model_routes.backend_ids` or `fallback_backend_id`, do not render an active destructive Delete button.
- Show clear text such as `Used by 1 route: deepseek-v4-pro`.
- Either disable the button with a useful tooltip/title or replace it with `View routes`.
- Keep the API 409 protection, but the Portal must not let Admin click a destructive action that is known to fail.

Acceptance condition:

- In Provider table, `DeepSeek V4 Pro` Delete is disabled or absent while `deepseek-v4-pro` route exists.
- `bash swarm/scripts/portal_logic_acceptance.sh` passes this provider guard.

### 2) Provider Delete after edit/save is not reliably clickable

Focused test reproduced this outside the canonical gate:

```text
before delete wait 500 class= flash
click failed after wait 500 locator.click: Timeout 3000ms exceeded.
before delete wait 1800 class=
click failed after wait 1800 locator.click: Timeout 3000ms exceeded.
before delete wait 5000 class=
click failed after wait 5000 locator.click: Timeout 3000ms exceeded.
{
  "exists": true,
  "dialogs": []
}
```

The row and Delete button are visible, but a normal Playwright click cannot trigger the button even after the flash class is removed. This is exactly the class of UI bug the user is seeing: CRUD looks present but interaction is not reliable.

Required fix:

- Make Provider table action buttons normal, stable clickable elements after create/edit rerender.
- Check CSS/table/sticky/actions/animation. The current row flash or table layout is likely leaving the action cell in a bad hit-test/stability state.
- Do not solve this with test `force: true`; a normal click must work.
- After fix, verify: create provider -> edit same provider -> click Delete -> confirm dialog appears -> toast `Provider deleted` -> backend disappears from API.

Acceptance condition:

- Canonical gate passes `unused Provider delete through UI removes backend`.
- Manual/focused Playwright normal click after edit opens the confirm dialog.

### 3) Team `budget:null` still does not clear budget in backend/API

Evidence from gate:

```json
{
  "api": {
    "name": "pw-logic-25296089-team-edited",
    "budget": {
      "period": "month",
      "max_tokens": 1000000,
      "max_usd_cents": null,
      "per_model": {}
    }
  },
  "patchResponse": { "id": 12 }
}
```

Required fix:

- Fix `PATCH /admin/teams/{id}` server-side.
- It must distinguish:
  - budget field omitted: leave existing budget unchanged.
  - budget field explicitly set to `null`: set DB budget column to SQL `NULL`.
  - budget object: validate and store object.
- UI-only changes are not enough. The canonical gate calls the API directly and still fails.

Acceptance condition:

- POST team with budget object.
- PATCH same team with `{ "budget": null }`.
- GET teams returns `budget: null`.

### 4) Enabled `deepseek-v4-pro` route does not work through router

Evidence from gate:

```json
{
  "model": "deepseek-v4-pro",
  "smoke": {
    "status": 503,
    "body": "no healthy backend available",
    "key_prefix": "lc-27361"
  },
  "route": {
    "model_name": "deepseek-v4-pro",
    "backend_ids": [19],
    "provider_model_name": "deepseek-v4-pro",
    "enabled": true,
    "auth_mode": "bearer",
    "protocol": "openai_chat"
  }
}
```

DB root cause observed:

```text
backends.id=19
backends.name=DeepSeek V4 Pro
backends.api_key_ref=file:/tmp/brigto-data/provider_keys/19.key
model_routes.model_name=deepseek-v4-pro
model_routes.provider_key_ref=NULL
```

Inside the router container there is no provider key file under `/tmp/brigto-data/provider_keys` or `/var/lib/brighto-router/provider_keys`, so config cannot resolve the backend key and the route has no healthy backend.

Required fix:

- Do not leave any enabled route pointing at an unresolved provider credential.
- When Admin enters a route/provider key in the Route wizard, store it in a path mounted into Docker and resolvable inside the router container, or store `provider_key_ref` consistently for the route.
- Do not store runtime key paths under container-local `/tmp` unless that path is backed by the Docker volume and survives recreate.
- If `.model` is the source for dev/test keys, import it into the same persisted secret location used by Docker runtime. Do not assume `.model` magically resolves at runtime.
- Add `Test endpoint` before enabling a route. Saving a disabled draft can be allowed, but an enabled route must pass smoke through `/v1/chat/completions` before it is considered production-ready.

Acceptance condition:

- `deepseek-v4-pro` returns HTTP 200 through router with an enabled client key.
- Canonical route smoke passes for all enabled routes.

## Required acceptance workflow before saying done

1. Apply fixes.
2. Rebuild image from source:

```bash
docker build -t thusinh1969/brighto_airouter:v1 .
```

3. Recreate live router:

```bash
docker compose up -d --force-recreate router
```

4. Wait for healthy:

```bash
docker inspect --format '{{.State.Health.Status}}' brighto-airouter-router-1
```

5. Run the canonical gate:

```bash
bash swarm/scripts/portal_logic_acceptance.sh
```

6. Only claim done if the script exits 0 and its `summary.json` has `"result": "PASS"`.

Do not bypass the route smoke with `BRIGHTO_SKIP_ROUTE_SMOKE=1` for final acceptance.
