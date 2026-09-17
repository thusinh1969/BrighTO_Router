# CODEX audit — mobile sidebar backdrop polish

Date: 2026-09-18
Role: Codex auditor/implementer
Scope: Portal shell mobile navigation polish

## Root cause

The mobile sidebar opened without a true page backdrop. Users had no obvious click-outside target, body scroll stayed active, and the full-page audit did not prove close behavior. This is a polish and usability gap for the Portal shell.

## Fix applied

- Added a dedicated mobile sidebar backdrop outside the sidebar.
- Locked body scroll while mobile navigation is open.
- Added click-outside close through the backdrop.
- Added Escape close and desktop-resize cleanup.
- Added an accessible label for the hamburger button.
- Extended the full-page Playwright audit to prove mobile sidebar open, backdrop active area, backdrop close, body scroll lock, and Escape close.

## Verification

Commands run against the live HTTPS container at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_logic_acceptance.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
```

Results:

- `portal-full-page-audit`: PASS
- `portal-modal-surface-audit`: PASS
- `portal-logic-acceptance`: PASS
- `portal-user-journey-audit`: PASS
- `portal-polish-audit`: PASS

