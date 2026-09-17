# CODEX CANONICAL PORTAL LOGIC GATE — current live FAIL

Date: 2026-09-17  
Runtime tested: live Docker HTTPS portal at `https://127.0.0.1:18443` after rebuild/recreate.  
Canonical gate added:

```bash
bash swarm/scripts/portal_logic_acceptance.sh
```

Latest artifact:

```text
swarm/out/playwright/20260917-125521-portal-logic-acceptance/summary.json
```

Verdict: **FAIL. DeepSeek must not write “done”, “green”, or “closed” until this canonical gate passes and Codex reruns it on live Docker.**

## Rule from now on

Do not claim Portal done from static source checks or from a narrow polish gate. Done means:

1. Build Docker image.
2. Recreate live router container.
3. Run static gate.
4. Run polish gate.
5. Run canonical logic gate.
6. Codex reruns the same checks on live Docker and confirms.

Commands:

```bash
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
python3 swarm/scripts/portal_static_gate.py
bash swarm/scripts/portal_polish_audit.sh
bash swarm/scripts/portal_logic_acceptance.sh
```

## What the canonical gate covers

`swarm/scripts/portal_logic_acceptance.sh` is now the single shared logic/UX/API acceptance gate. It does all of this in one run and cleans its own test data before/after:

- Admin login over live HTTPS.
- Formatter/density live runtime check.
- Test data pollution check: no stale `pw-*`, `crud-*`, `verify-*` providers/routes/teams/keys.
- Provider page copy: key must belong to route, not provider.
- Provider cloud row must not have active `Load models` without key.
- Provider Edit action visible at 1280px and 1024px.
- Provider modal has `Provider Type`.
- Provider edit persists and visibly shows Weight/Max concurrent.
- Route provider dropdown has no duplicate display names and no stale test rows.
- Cloud blank-key Load models must be locally blocked with friendly validation.
- Route create persists and visibly shows provider model/auth/context/max output/prices.
- Team explicit `budget:null` must clear budget.
- API key reveal/Edit must work and persist metadata.
- User login must hide admin menus and land on Dashboard.
- Browser console must have no errors in normal flows.

## Live passes after latest DeepSeek patch

After commit `a821583` and Docker rebuild, these now pass:

- Source SHA == live HTML SHA.
- Provider-row `Load models` for cloud providers is removed from live UI.
- Provider copy now says API key is configured in Model Route.
- Provider Edit is visible at 1280 and 1024 viewport widths.
- Provider modal has `Provider Type`.
- Provider edit shows/persists Weight and Max concurrent.
- Route table shows/persists provider model, auth, context, max output, prices.
- Route provider dropdown no longer has duplicate display names after DB cleanup.
- API key reveal/Edit works and persists owner/RPM/concurrency.
- Static gate PASS.
- Polish gate PASS.

## Current FAIL 1 — Route wizard blank cloud key leaks 401/502

Flow:

1. Models & Routes → Create model route.
2. Show disabled providers.
3. Select `openai`.
4. Protocol `openai_chat`.
5. Authentication `bearer`.
6. Leave Provider API key blank.
7. Click `Load models`.

Current toast:

```text
Load models: 502 provider models endpoint returned 401 Unauthorized
```

Required fix:

- Frontend: before calling `/admin/routes/preview-models`, if auth requires key and provider key field is blank, show: `Enter API key first`.
- Backend: `/admin/routes/preview-models` must also return clear `400` for blank key when `auth_mode=bearer` or `auth_mode=anthropic`.
- Do not make outbound OpenAI/Anthropic/etc requests in a known blank-key state.

Acceptance:

- Same flow shows friendly local validation.
- No browser console `502`.
- No provider `401 Unauthorized` appears.

## Current FAIL 2 — Team explicit budget:null does not clear budget

Canonical API test:

1. POST team with monthly `max_tokens=1000000`.
2. PATCH same team with:

```json
{ "name": "...-edited", "budget": null, "enabled": true }
```

Current GET still returns old budget:

```json
{
  "budget": {
    "period": "month",
    "max_tokens": 1000000,
    "per_model": {}
  }
}
```

Required fix:

- Backend PATCH `/admin/teams/{id}` must distinguish omitted `budget` from explicit JSON `null`.
- Explicit `budget:null` must set PostgreSQL `teams.budget = NULL`.
- Frontend Team modal must also ignore/clear hidden Advanced JSON when visible Budget Type is Unlimited.

Acceptance:

- Team token budget → Unlimited → Save → F5 → API returns `budget:null`, UI row says `Unlimited`.

## Current FAIL 3 — User login retains stale Admin page title

Canonical test signs out from Admin while active nav is API Keys, then signs in as User.

Current state:

```json
{
  "providers": "none",
  "models": "none",
  "keys": "none",
  "title": "API Keys"
}
```

Admin menus are hidden, but title remains `API Keys`. This is confusing and wrong.

Required fix:

- On mode switch/login/restore, reset active view to Dashboard when entering User mode.
- Ensure `#page-title` and `.nav.active` match the view actually rendered.

Acceptance:

- Sign out from Admin while on API Keys.
- Sign in as User.
- Title is `Dashboard`; admin nav buttons are hidden; content is user dashboard.

## Current FAIL 4 — Runtime console error caused by blank-key Load models

Console error observed:

```text
Failed to load resource: the server responded with a status of 502 ()
```

This should disappear once blank-key model loading is blocked before request.

## Test data status

After the canonical run and cleanup:

- `/admin/backends`: 12 rows, 0 `pw/crud/verify` junk rows.
- `/admin/teams`: 2 rows, 0 `pw/crud/verify` junk rows.
- `/admin/keys`: 4 rows, 0 `pw/crud/verify` junk rows.
- `/admin/routes`: 3 rows, 0 `pw/crud/verify` junk rows.

Keep it that way. Any gate that leaves test rows behind is failed.

## Required DeepSeek response format

When fixed, DeepSeek must write one audit file containing:

- commit hash(es),
- exact Docker rebuild/recreate evidence,
- `portal_static_gate.py` result,
- `portal_polish_audit.sh` result,
- `portal_logic_acceptance.sh` result,
- summary JSON artifact path,
- list of test records created and cleaned.

No “done/green/closed” language without those facts.
