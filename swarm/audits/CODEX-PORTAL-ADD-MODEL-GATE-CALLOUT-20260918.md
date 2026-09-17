# CODEX audit — Add Model save gate callout

Date: 2026-09-18
Scope: Admin Portal `Add model` modal, desktop and mobile.

## Finding
The Add Model modal had awkward gate copy (`Test connection to enable Save enabled`) and the mobile sticky action footer could cover the model map / optional limits drawer. This made the main production safety rule less clear: enabled routes require a real connection test.

## Fix applied
- Replaced the plain hint with a visible `connection-status` callout.
- Updated gate copy to: `Run Test connection before saving an enabled route.`
- Converted pass/fail test state into green/red status callouts.
- Removed the old mobile auto-scroll that clipped the modal title.
- Compacted mobile Add Model spacing so the title, status gate, model map, optional drawer, and sticky actions are all visible without overlap.
- Updated modal Playwright audit selectors from the old `.hint` text to `.connection-status`.

## Evidence
- `python3 swarm/scripts/portal_static_gate.py` passed, including inline JavaScript parsing and the new gate-copy check.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh` passed with 0 failures and 0 console errors.
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` passed with 0 failures and 0 console errors.
- Visual screenshots inspected:
  - `swarm/out/playwright/20260918-064225-portal-modal-surface-audit/mobile-add-model.png`
  - `swarm/out/playwright/20260918-064225-portal-modal-surface-audit/desktop-add-model.png`
