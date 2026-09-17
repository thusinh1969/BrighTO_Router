# CODEX Portal API key model scope picker — 2026-09-17

## Result

PASS. The API key modal no longer asks admins to type comma-separated model names. It now uses a guided model access picker:

- `All models` means the key can call every enabled model route.
- `Restrict to selected models` shows available public model route names as selectable chips.
- Restricted mode cannot save with zero selected models.
- Backend payload is unchanged: `allowed_models: []` for all models, or an exact array of selected public model names.

## Why this was needed

The old API key UX exposed `Allowed models (comma-separated, empty = all)`. That is not professional for an admin portal because it is easy to mistype a route name and hard to audit visually. The admin should choose from existing public route names.

## Files changed

- `static/index.html`
  - Replaced the comma-separated allowed-model input in `openKeyModal()` with a segmented picker.
  - Fixed chip checkbox sizing so the picker stays compact and aligned.
  - Preserved the existing API contract.

- `swarm/scripts/portal_logic_acceptance.mjs`
  - Canonical acceptance now creates an API key through restricted model picker.
  - Verifies `allowed_models` persisted exactly as the selected route.

- `swarm/scripts/portal_logic_acceptance_instrumented.mjs`
  - Updated the instrumented CRUD flow to use the picker instead of the removed text input.

- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Fails if New Key modal regresses to comma-separated copy.
  - Fails if guided model access picker disappears.

## Verification

Static and syntax gates:

```bash
python3 swarm/scripts/portal_static_gate.py
node --check /tmp/brighto-portal.js
node --check swarm/scripts/portal_modal_surface_audit.mjs
node --check swarm/scripts/portal_logic_acceptance_instrumented.mjs
node --check swarm/scripts/portal_logic_acceptance.mjs
git diff --check
```

Portal Playwright suite, live HTTPS runtime:

```bash
portal_login_audit PASS
portal_empty_state_audit PASS
portal_logic_acceptance PASS
portal_visual_audit PASS
portal_polish_audit PASS
portal_full_page_audit PASS
portal_modal_surface_audit PASS
portal_user_journey_audit PASS
```

Evidence log prefix:

```text
swarm/out/*-key-scope-picker-222017.log
```
