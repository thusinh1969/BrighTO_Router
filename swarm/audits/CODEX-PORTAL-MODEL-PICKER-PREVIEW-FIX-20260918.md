# CODEX audit — Add model picker preview consistency — 2026-09-18

## Problem

In the Add model wizard, selecting a model from the Load models picker updated the hidden form state, but the public-to-provider mapping preview could stay stale until the user typed again.

That preview is important because it tells the admin exactly what clients will send and what the provider receives. If it lags behind the picker selection, the wizard feels unreliable.

## Fix

Changed `static/index.html`:

- Added `applyPickedModel(chosen)` inside `openRouteModal()`.
- The helper now updates:
  - Provider model input.
  - Public model input when empty.
  - Save-enabled gate text.
  - Mapping preview via `updateModelMap()`.
- `pickModelModal()` now receives this helper directly, so both click and double-click paths use the same state transition.

Updated `swarm/scripts/portal_modal_surface_audit.mjs`:

- Intercepts `/admin/routes/preview-models` with a mock provider model list.
- Clicks `Load models`.
- Selects one model in the picker.
- Verifies the provider input, public input, mapping preview, and Save-enabled gate are all updated.
- Runs this on desktop and mobile modal surfaces.

## Verified

Commands run against live HTTPS portal:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_logic_acceptance.sh
```

Result: both PASS.

Key evidence from modal audit:

- `desktop-add-model-picker-preview.providerModel = desktop-add-model-picker-model`
- `desktop-add-model-picker-preview.publicModel = desktop-add-model-picker-model`
- `desktop-add-model-picker-preview.mapText` contains the selected model on both sides.
- Same behavior verified for mobile.

## Verdict

The Add model wizard now keeps picker selection, visible inputs, and mapping preview in one consistent state. This closes a small but real trust bug in the most important setup flow.
