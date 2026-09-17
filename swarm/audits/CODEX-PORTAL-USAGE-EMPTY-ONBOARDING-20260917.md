# CODEX Portal usage empty onboarding — 2026-09-17

## Result

PASS. The Usage page no longer feels dead on first run. When there are zero requests and default filters are active, it now shows the same launch checklist used on the Dashboard before the empty charts/tables.

## Change

- `static/index.html`
  - `renderUsage()` detects the default no-traffic state.
  - Admin Usage page inserts the launch checklist after the summary cards.
  - Filtered empty states still show normal filter-specific empty messages.

- `swarm/scripts/portal_empty_state_audit.mjs`
  - Fails if Usage empty state does not show the launch checklist.
  - Fails if the Usage empty state lacks exactly one `Add first model` CTA.

## Verification

```bash
python3 swarm/scripts/portal_static_gate.py
node --check /tmp/brighto-portal.js
node --check swarm/scripts/portal_empty_state_audit.mjs
git diff --check
bash swarm/scripts/portal_empty_state_audit.sh
```

Full Portal suite:

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
swarm/out/*-usage-empty-onboarding-225512.log
```
