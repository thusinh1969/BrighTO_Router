# CODEX URGENT AUDIT — Provider Edit exists in DOM but is effectively hidden/off-screen

Date: 2026-09-17  
Runtime tested: live Docker HTTPS portal at `https://127.0.0.1:18443`  
Method: focused Playwright on Providers menu and first row Edit modal.  
Artifact: `swarm/out/playwright/20260917-121841-provider-edit-ux/summary.json`

Verdict: **USER IS RIGHT. Provider Edit technically exists, but the UI makes it effectively invisible/untrustworthy. Fix now.**

## Runtime evidence

At viewport `1280 x 800`:

```json
{
  "wrapClientWidth": 932,
  "wrapScrollWidth": 1342,
  "firstRowButtons": [
    { "text": "Load models", "x": 1516, "w": 110 },
    { "text": "Edit", "x": 1516, "w": 51 },
    { "text": "Delete", "x": 1516, "w": 68 }
  ]
}
```

At viewport `1024 x 760`:

```json
{
  "wrapClientWidth": 676,
  "wrapScrollWidth": 1342,
  "horizontalScrollNeeded": true,
  "firstRowButtons": [
    { "text": "Load models", "x": 1106, "right": 1216 },
    { "text": "Edit", "x": 1106, "right": 1157 },
    { "text": "Delete", "x": 1106, "right": 1174 }
  ]
}
```

This means the Edit action is to the right of the visible area. The copied text includes “Edit”, but the user may not see or reach it without horizontal scrolling. That is not acceptable for an admin portal.

## Confirmed behavior

Playwright can force-click the off-screen button. It opens this modal:

```text
Edit provider · openai
NAME
BASE URL
WEIGHT
MAX CONCURRENT (0 = UNLIMITED)
FORMAT
OpenAI-compatible
Anthropic Messages
ENABLED
Cancel
Save
```

So the backend route and modal exist. The failure is product/UI design.

## Bugs to fix now

### 1. Make Provider row actions always visible

Required fix:

- Make the action column sticky on the right, or move primary actions into visible columns.
- The user must see `Edit` without horizontal scrolling at 1024px and 1280px widths.
- Keep table horizontal scroll only for low-priority fields, not for actions.

Acceptance:

- At viewport 1024x760, first provider row shows visible `Edit` button inside viewport.
- At viewport 1280x800, first provider row shows visible `Edit` button inside viewport.
- Playwright must assert button bounding box `x >= 0 && right <= viewportWidth`.

### 2. Remove wrong Provider key copy

Current Provider page says:

```text
Set provider API keys in the shell (<b>./start.sh set-key <name> <key></b>) or edit the key reference here. Only configured / missing status is shown — never the secret.
```

This is false for the current UI: Provider modal has no key/reference field, while Route modal has provider key. This copy is causing exactly the confusion the user reported.

Required fix:

- If route owns provider API keys, Provider page must say:
  - Provider stores endpoint/template/capacity only.
  - API key is entered in Model Route.
- Do not mention editing key reference in Provider unless Provider modal actually has that field.

Acceptance:

- Providers page contains no `set-key`, no `edit key reference here`, no contradictory credential copy.

### 3. Add Provider Type, not raw “Format” only

Current Provider modal labels:

```json
["Name", "Base URL", "Weight", "Max concurrent (0 = unlimited)", "Format", "Enabled"]
```

This is too low-level and does not match admin mental model.

Required fix:

- Replace or supplement `Format` with `Provider Type`:
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
- Selecting a type should fill sensible default URL and allowed protocol set.
- Keep backend DB simple if needed; this can map to existing `format` and route `protocol`.

Acceptance:

- Provider modal has a clear Provider Type dropdown.
- OpenAI row no longer advertises impossible/irrelevant protocols like Local Chat as if it were normal.

### 4. Provider table must show fields changed by Edit

Provider Edit lets admin modify Weight and Max concurrent, but table hides both. That makes Save look broken even when API updated correctly.

Required fix:

- Show Weight and Max concurrent in the Provider table, or show them in an always-visible compact details line.
- After Save, highlight the changed row briefly.

Acceptance:

- Edit provider weight `1 -> 7`; after Save, visible row shows `7`.
- Edit max concurrent `0 -> 3`; after Save, visible row shows `3`.

## Do not close until live Docker passes

After patch:

```bash
docker build -t thusinh1969/brighto_airouter:v1 .
docker compose up -d --force-recreate router
```

Then run focused Playwright and attach evidence:

- Provider Edit button visible at 1024 and 1280 widths.
- Provider modal has Provider Type.
- Wrong key copy removed.
- Edited fields are visible after Save.
