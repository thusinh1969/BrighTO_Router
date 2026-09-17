# CODEX Portal Add Model optional drawer — 2026-09-17

## Result

PASS. Add Model is now shorter and more usable. The primary create flow is visible without scrolling on desktop:

1. Pick provider and URL.
2. Load or type provider model.
3. See public-to-provider model mapping.
4. Test connection.
5. Save enabled or save disabled.

Optional context, output cap, pricing, fallback provider, and timeout settings are collapsed behind `Optional limits and pricing`.

## Why this matters

Add Model is the most important admin task. The previous form showed every optional field upfront, pushing `Test connection` and `Save enabled` below the first viewport on desktop and far below on mobile. That made the product feel heavier than it is.

## Files changed

- `static/index.html`
  - Added an `Optional limits and pricing` drawer.
  - New routes keep the drawer closed by default.
  - Editing an existing route opens the drawer automatically when optional settings are already set.
  - Restored desktop action visibility while keeping mobile modal internal scrolling.

- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Fails if Add Model loses the mapping preview.
  - Fails if desktop Add Model primary actions are not visible.
  - Keeps mobile viewport and overlap checks.

## Verification

```bash
python3 swarm/scripts/portal_static_gate.py
node --check /tmp/brighto-portal.js
node --check swarm/scripts/portal_modal_surface_audit.mjs
git diff --check
bash swarm/scripts/portal_modal_surface_audit.sh
```

Live Portal checks:

```bash
portal_logic_acceptance PASS
portal_modal_surface_audit PASS
portal_full_page_audit PASS
portal_user_journey_audit PASS
```

Evidence log prefix:

```text
swarm/out/*-add-model-optional-drawer-224434.log
```
