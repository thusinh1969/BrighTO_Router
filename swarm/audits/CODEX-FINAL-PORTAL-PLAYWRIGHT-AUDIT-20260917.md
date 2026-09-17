# CODEX FINAL PORTAL PLAYWRIGHT AUDIT — DeepSeek action list

Date: 2026-09-17  
Runtime tested: live Docker HTTPS portal at `https://127.0.0.1:18443`  
Method: Playwright Chromium against the real running portal, plus Admin API verification after each Save/Delete.  
Artifacts:

- Full admin flow: `swarm/out/playwright/20260917-120946-full-portal-audit-noscreenshot/summary.json`
- Current polish gate: `swarm/out/playwright/20260917-121350-portal-polish-audit/summary.json`
- Focused User Portal flow: `swarm/out/playwright/20260917-121131-user-portal-focused/summary.json`
- Focused Route/Provider delete + route Load models: `swarm/out/playwright/20260917-121248-delete-load-focused/summary.json`

Verdict: **NOT READY for release. Backend CRUD mostly works, but Admin product logic and UI clarity are still below SOTA. Fix the items below; do not argue from source-only checks. Build Docker and retest live.**

## What passed in live Playwright

- HTTPS Admin login works.
- Admin navigation opens all menus: Dashboard, Providers, Models & Routes, Teams, API Keys, Usage, Settings.
- Admin session persists after F5.
- Settings has Font size and Density controls; compact density applied in live browser when set.
- Provider Add works: UI row appears and Admin API persists the created backend.
- Provider Edit works for a new provider: API updated `name`, `base_url`, `format`, `weight`, `max_inflight`, `enabled`; the visible row updated for name/base/protocol/status.
- Provider Load models works against local llama.cpp backend `local-llama-qwen`, returning `qwen3.8-flash-next`.
- Provider Delete UI works and refreshes the table.
- Route modal Load models works against local llama.cpp and populates/selects `qwen3.8-flash-next`.
- Route Create works and persists `provider_model_name`, `auth_mode`, `protocol`, context, max output, prices, timeout.
- Route Edit works without duplicating rows.
- Route Delete UI works and refreshes the table.
- Team Create works.
- Team Edit works for name/enabled state.
- API Key Create reveals the full key in the modal.
- API Keys table reveals full stored keys for revealable keys.
- API Key Disable works.
- User Portal login works with an enabled client API key.
- User Portal hides Admin-only menus.
- User Portal Usage opens.
- User Portal session persists after F5.

## Logic blockers DeepSeek must fix

### 1. Live Docker is stale versus current source

Current source `static/index.html` has a newer formatter than the live Docker page, but the browser still receives the old formatter.

Evidence captured after audit:

- Source `static/index.html` SHA prefix: `b3f05de74b3cf8b3`
- Live `https://127.0.0.1:18443/` SHA prefix: `f6fe1f6ecf2c049d`
- Source `fmt()` removes `.0` via `replace(/\.0([KMB])$/, "$1")`.
- Live Docker `fmt()` still returns `1.0K`, `50.0K`, `1.0M`.

Required fix:

- After every portal source change, rebuild and restart the Docker image actually serving the portal.
- Acceptance must be against the live Docker URL, not only `python3 swarm/scripts/portal_static_gate.py`.
- Run:
  - `docker build -t thusinh1969/brighto_airouter:v1 .`
  - `docker compose up -d --force-recreate router`
  - then Playwright against `https://127.0.0.1:18443`.

Acceptance:

- In live browser JS: `fmt(1000) === "1K"`, `fmt(50000) === "50K"`, `fmt(1000000) === "1M"`.
- `curl -ks https://127.0.0.1:18443/` contains the same formatter source as `static/index.html`.

### 2. Provider mental model is still confusing

User requirement: Provider is a provider template / endpoint. Route owns the provider API key because one provider can have many model routes with different API keys if needed.

Current Provider page still says:

> Set provider API keys in the shell (`./start.sh set-key <name> <key>`) or edit the key reference here.

But the Provider modal has no key field. The Route modal has the key field. This contradiction is exactly why the user feels the flow is broken.

Required fix:

- Decide and implement one product rule: **route owns provider API key**.
- Provider page copy must say: Provider stores display name, endpoint URL, provider type/protocol family, enabled flag, weight, max concurrency. API key is entered when creating/editing a model route.
- Remove all “set provider API key in shell” and “edit key reference here” text from Provider UI if route owns the key.
- In Route modal, label it clearly: “API key for this route/provider model”. Blank on edit = keep existing.

Acceptance:

- A first-time admin can understand: Providers = endpoint templates, Routes = actual usable models and credentials.
- No UI text says Provider key is editable in Provider modal unless that field actually exists there.

### 3. Provider type/protocol model is too weak

Current backend has only `format=openai|anthropic`, while UI shows broad protocol options. For an OpenAI-like provider row it shows: OpenAI Chat, Completions, Embeddings, Local Chat, Custom. This is not professional; it makes OpenAI, custom local, and embeddings look interchangeable.

Required fix:

- In UI, add a simple Provider Type dropdown when adding/editing provider:
  - OpenAI
  - Anthropic
  - Gemini OpenAI-compatible
  - DeepSeek
  - Kimi
  - Qwen
  - Z.AI / GLM
  - OpenRouter
  - Meta Muse
  - Local OpenAI-compatible
  - Custom OpenAI-compatible
  - Custom Anthropic-compatible
- Selecting a type fills sensible default base URL and allowed protocols.
- Keep DB simple if needed: this can be UI metadata mapped to current `format` plus route `protocol`; do not add heavy provider registry if not needed.
- Do not show impossible protocols for the selected provider type.

Acceptance:

- OpenAI provider does not show “Local Chat” as a normal option.
- Anthropic provider shows only Anthropic Messages unless explicitly custom.
- Local llama.cpp defaults to OpenAI-compatible chat + no auth.

### 4. Route modal allows invalid protocol/auth combinations

Playwright created a route with `protocol=anthropic_messages` and `auth_mode=none` because the modal allows auth to be manually changed after protocol selection. That combination is nonsensical for normal Anthropic cloud and dangerous for production config quality.

Required fix:

- Validate compatibility before Save:
  - `anthropic_messages` defaults to `auth_mode=anthropic`; `none` only allowed for an explicit custom local/no-auth provider type.
  - `local_openai_chat` defaults to `auth_mode=none`.
  - OpenAI-compatible cloud protocols default to bearer.
- If admin chooses an invalid combination, block Save with a clear error.

Acceptance:

- Cannot save Anthropic cloud route with `auth_mode=none`.
- Cannot save local no-auth route under Anthropic Messages unless provider type explicitly supports that.

### 5. Provider Load models quick-create has a state bug in source

In `static/index.html`, Provider → Load models → click model uses:

```js
routes=api("/admin/routes","GET");
```

It does not `await`, and it does not rerender. This can turn `routes` into a Promise until the next full refresh.

Required fix:

```js
routes = await api("/admin/routes", "GET");
rerender();
```

or remove quick-create from Provider Load models and force route creation through the full Route modal, which is safer and clearer.

Acceptance:

- Clicking a model from Provider Load models either opens prefilled Route modal or creates a route and immediately refreshes the Models table.
- `routes` must always remain an array.

### 6. Team budget “Unlimited” edit does not clear existing token budget

Playwright edited a test team from token budget to Unlimited. API result still showed the old token budget. The row can say Unlimited intent in modal, but persisted data remains token budget.

Evidence from full audit:

```json
{
  "team": {
    "name": "pw-full-21789388-team-edited",
    "budget": { "period": "month", "max_tokens": 1000000 },
    "enabled": false
  }
}
```

Required fix:

- If Budget type = Unlimited, send `budget: null` explicitly.
- Backend PATCH must treat explicit JSON null as clear budget, not “field omitted”.

Acceptance:

- Create team with 1M tokens, edit to Unlimited, save, refresh/F5.
- Admin API returns `budget: null` and table shows Unlimited.

### 7. API key edit is missing

API Keys support Create, Reveal, Disable only. User asked to check edit API key; there is no Edit flow for expiry, budget, RPM, concurrency, owner, team, or allowed models.

Required fix, no over-engineering:

- Add “Edit” for API key metadata:
  - owner
  - team
  - allowed models
  - expiry
  - requests per minute
  - concurrency
  - key budget
  - enabled flag
- Do not regenerate the secret on edit.
- If edit is intentionally not supported, UI must say “Disable + create replacement” clearly. Current UI does not.

Acceptance:

- Create key, edit expiry/RPM/budget, save, F5, API shows exactly one same key row updated.

## UI blockers DeepSeek must fix

### 1. Provider edit appears broken because edited fields are hidden

Provider Edit persisted in API, but table only shows: ID, Provider, Status, Protocols, Base URL. It hides Weight and Max concurrent, so user changes those fields and sees no visible change.

Required fix:

- Add compact columns or details drawer for:
  - Weight
  - Max concurrent
  - Provider type
  - Key policy: “route-owned key” or “provider-owned key”, whichever final logic chooses
- After Save, highlight the changed row for 1–2 seconds and show “Provider saved”.

Acceptance:

- Edit weight from 1 → 7 and max concurrent from 0 → 3; after Save the visible row proves those values changed.

### 2. Models & Routes table hides production-critical fields

Route Create/Edit persists these fields, but the table hides most of them:

- provider model name
- auth mode
- context window
- max output tokens
- input price per 1M tokens
- output price per 1M tokens

Current row only shows public model, backends, protocol, fallback, timeout. That is not enough for an admin to trust pricing/routing.

Required fix:

- Show at least:
  - Public model
  - Provider
  - Provider model
  - Protocol
  - Auth
  - Context
  - Max output
  - Price in / out per 1M
  - Timeout
  - Enabled
- If the table becomes wide, use horizontal scroll or a compact details drawer.

Acceptance:

- Create/edit a route with provider model `qwen3.8-flash-next`, context `8192`, max output `64`, prices `0.10/0.20`; after Save those values are visible without reopening modal.

### 3. Provider list/dropdowns are polluted and hard to use

Live DB had many old test providers before cleanup. I removed `pw-polish-*` and `pw-full-*` test artifacts created by audits, but the UI remains vulnerable: dropdowns list every backend flat, including duplicates like repeated “Local Qwen”.

Required fix:

- Provider dropdowns must show `name + base URL short + ID` or enforce unique display names.
- Add search/filter when the list grows.
- Hide disabled providers by default in route creation, with a “show disabled” toggle.
- Do not let audit scripts leave test records behind.

Acceptance:

- Route provider dropdown is unambiguous even with duplicate provider names.
- Test runs cleanup their own providers/routes/keys/teams.

### 4. Settings UI still orders density wrong

Default is compact in current source/runtime state, but Settings still lists Comfortable before Compact. This makes the default look accidental and makes the user think nothing changed.

Required fix:

- Put Compact first, Comfortable second.
- Label default clearly: “Compact (default)” and “Comfortable”.

Acceptance:

- Fresh browser opens compact by default and Settings visibly shows Compact selected first.

### 5. Use consistent compact count formatting everywhere

Current source has the right direction, but live Docker was stale during audit. After rebuild, all count displays must use no-useless-decimal K/M/B.

Required examples:

| Raw | Display |
|---:|---:|
| 999 | `999` |
| 1,000 | `1K` |
| 1,234 | `1.2K` |
| 50,000 | `50K` |
| 1,000,000 | `1M` |
| 12,345,678 | `12.3M` |
| 1,000,000,000 | `1B` |
| 2,500,000,000 | `2.5B` |

Acceptance:

- Dashboard, Usage cards, Usage grouped tables, Request logs, Team budget, chart axis, Settings max body bytes all follow the same formatter.

## Final acceptance checklist before telling user “fixed”

Run against live Docker only:

```bash
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
python3 swarm/scripts/portal_static_gate.py
bash swarm/scripts/portal_polish_audit.sh
```

Then run a manual/Playwright flow and attach evidence:

1. Fresh browser Admin login over HTTPS.
2. F5 keeps Admin session.
3. Settings opens; Compact default selected first; Small/Normal/Large changes display.
4. Provider Add/Edit/Delete: edit name/base/type/weight/max concurrent/enabled; visible row shows changed values.
5. Provider Load models against local llama.cpp returns `qwen3.8-flash-next` and does not corrupt `routes` state.
6. Route Create/Edit/Delete: load model list, choose exactly one model, enter route key, set context/max output/prices; row shows all important values; edit does not duplicate.
7. Team Create/Edit: switch token budget to Unlimited and verify API returns `budget: null`.
8. API Key Create/Reveal/Edit/Disable: edit metadata without regenerating key.
9. Usage: Tok/s visible, token counts compact, filters work.
10. User login with client key, F5 keeps session, admin menus hidden.

Do not close this as done until live Docker passes these steps. Static source checks are not enough.
