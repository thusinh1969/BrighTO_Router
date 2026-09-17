# CODEX URGENT AUDIT — Provider/Route credential logic still wrong

Date: 2026-09-17  
Runtime tested: live Docker HTTPS portal at `https://127.0.0.1:18443`  
Method: Playwright against real Admin Portal + Admin API inspection.  
Artifacts:

- `swarm/out/playwright/20260917-124632-provider-route-logic/summary.json`
- `swarm/out/playwright/20260917-124748-route-dropdown-disabled/summary.json`
- `swarm/out/playwright/20260917-124832-route-load-blank-key/summary.json`

Verdict: **FAIL. The visible polish improved, but Provider/Route credential logic is still not production-friendly. User is right.**

## Root product rule

We chose this rule earlier and the UI copy now says it:

> Provider = endpoint/template definition.  
> Model Route = actual routable model + provider model + API key/auth/pricing.

If that is the rule, then **Provider row must not try to Load models for cloud providers**, because it has no route API key to call OpenAI/Anthropic/etc.

## Bug 1 — Provider row Load models calls cloud provider without a key

Playwright clicked Providers → `openai` row → `Load models`.

Runtime toast:

```text
Loading models from openai …
Load models: 502 provider models endpoint returned 401 Unauthorized
```

This is exactly the user's complaint. The Provider screen says API key is entered per Route, but the Provider row still has `Load models` and it calls the cloud provider without a route key.

Required fix:

- Remove `Load models` from Provider rows for cloud/provider-template rows.
- Keep `Load models` only inside Route wizard, after admin enters the route/provider API key.
- If you keep Provider-row Load models at all, only enable it for local/no-auth providers and label it `Load local models`.
- Do not surface raw provider `401/502` to admin for an expected blank-key state.

Acceptance:

- Provider row `openai` has no active `Load models` button, or it is disabled with tooltip: `Create a route and enter API key first`.
- Provider row local llama.cpp can load `qwen3.8-flash-next` if no auth is required.

## Bug 2 — Route wizard Load models with blank key still leaks provider unauthorized

Playwright flow:

1. Models & Routes → Create model route.
2. Show disabled providers.
3. Select `openai (id 1)`.
4. Protocol `openai_chat`.
5. Authentication `bearer`.
6. Leave Provider API key blank.
7. Click `Load models`.

Runtime toast:

```text
Load models: 502 provider models endpoint returned 401 Unauthorized
```

Required fix:

- Validate locally before calling `/admin/routes/preview-models`:
  - If auth mode is `bearer` and provider key is blank: show `Enter API key first`.
  - If auth mode is `anthropic` and provider key is blank: show `Enter API key first`.
  - If auth mode is `none`, only allow for local/custom no-auth provider types.
- Backend should also reject preview-models with clear 400 for missing key when auth requires key.

Acceptance:

- Blank key never makes an outbound OpenAI/Anthropic/etc request.
- UI toast is friendly and deterministic, not `401 Unauthorized` or `502`.

## Bug 3 — Route wizard first-run provider dropdown says “No providers” even though templates exist

Admin API has 19 backend templates, but all are disabled. Route wizard hides disabled providers by default. On this DB, Create route opens with only:

```text
No providers
```

This is bad first-run UX. The product wants predefined providers so admin can choose OpenAI/Anthropic/Gemini/DeepSeek/etc and enter API key in route. Hiding all templates makes the route wizard look broken.

Required fix:

- If no enabled providers exist, show disabled/templates by default, or show a clear provider-template picker.
- Label the toggle as `Show disabled/templates`; default it on when active list is empty.
- Do not present `No providers` when provider templates exist.

Acceptance:

- Fresh install: Create model route offers OpenAI, Anthropic, Gemini, DeepSeek, Kimi, Qwen, Z.AI, OpenRouter, Meta Muse, Custom, Local/custom templates without requiring admin to hunt for a hidden toggle.

## Bug 4 — Show disabled dropdown is polluted with duplicates and vague names

After toggling Show disabled, Playwright saw:

```text
openai (id 1)
anthropic (id 2)
gemini (id 3)
deepseek (id 4)
kimi (id 5)
qwen (id 6)
zai (id 7)
openrouter (id 8)
meta-muse (id 9)
custom-openai (id 10)
local-llama-qwen (id 11)
Local Qwen (id 13)
local-llama-qwen-audit (id 14)
Local Qwen (id 15)
DeepSeek V4 Pro (id 16)
Local Qwen (id 17)
Local Qwen (id 18)
DeepSeek V4 Pro (id 19)
Local Qwen (id 20)
```

Confirmed duplicates:

- `Local Qwen` appears 5 times.
- `DeepSeek V4 Pro` appears 2 times.
- Local qwen endpoint should only be the user-provided llama.cpp endpoint: `http://127.0.0.1:8088/v1`.

Required fix:

- Clean seed/test data and make seeding idempotent by unique provider name or provider type + base URL.
- Keep exactly one local llama.cpp provider template for this machine unless admin explicitly creates another.
- Route provider dropdown option must include enough context:
  - provider name
  - type
  - short base URL
  - id only as secondary detail
- Example: `OpenAI · api.openai.com · id 1`, `Local llama.cpp · 127.0.0.1:8088/v1 · id 11`.

Acceptance:

- No duplicate `Local Qwen` or duplicate `DeepSeek V4 Pro` in route dropdown after fresh seed/current DB cleanup.
- User can identify exactly which provider they are selecting.

## Bug 5 — Team Unlimited clear still fails because hidden Advanced JSON wins

Current source has:

```js
else { payload.budget=null; } // Unlimited -> clear any existing budget
```

But the edit modal also preloads hidden Advanced JSON:

```js
if(tm.budget) bi.value=JSON.stringify(tm.budget)
```

Save logic checks `bi.value.trim()` first, so the hidden old budget is sent again even when Budget Type = Unlimited.

API-level test also proved backend PATCH with `budget:null` did not clear persisted budget:

```text
create team budget 1M -> PATCH { budget: null } -> GET still returns max_tokens: 1000000
```

Required fix:

Frontend:

- If Budget type is Unlimited, ignore/clear Advanced JSON before building payload.
- Do not let a hidden textarea override the visible Budget Type.

Backend:

- PATCH `/admin/teams/{id}` must distinguish omitted `budget` from explicit JSON null.
- Explicit `budget:null` must set DB `budget = NULL`.

Acceptance:

- Create team with token budget 1M.
- Edit to Unlimited.
- Save.
- F5.
- Admin API returns `budget:null` and UI row shows `Unlimited`.

## What is already OK and should not be broken

Latest runtime checks passed these after DeepSeek's recent patch:

- Provider Edit action is visible at 1280px and 1024px.
- Provider page wrong key copy was removed.
- Provider modal has Provider Type.
- Provider edit persists and displays Weight/Max concurrent.
- Route table now displays provider model/auth/context/max output/prices.
- API Key Edit exists and persists owner/RPM/concurrency.
- Compact formatter and compact density are live.

Do not regress these.

## Fix order

1. Remove/disable Provider-row cloud `Load models`.
2. Add blank-key validation to Route wizard and backend preview endpoint.
3. Fix route wizard provider/template dropdown first-run behavior.
4. Deduplicate/cleanup seeded providers.
5. Fix Team `budget:null` end-to-end.
6. Rebuild Docker and rerun live Playwright.

## Required live acceptance after patch

Run against live Docker, not static source only:

```bash
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
python3 swarm/scripts/portal_static_gate.py
bash swarm/scripts/portal_polish_audit.sh
```

Then prove with Playwright:

- Open Providers: OpenAI row does not offer active Load models without a key.
- Open Models & Routes → Create model route: provider picker is useful on fresh install/templates.
- Select OpenAI with blank key → Load models shows `Enter API key first`, no 401/502.
- Select local llama.cpp → Load models returns `qwen3.8-flash-next` with no key.
- Dropdown contains no duplicate Local Qwen / DeepSeek V4 Pro rows.
- Team token budget → Unlimited clears to API `budget:null`.
