# CODEX audit — Toast navigation polish

Date: 2026-09-18
Scope: `static/index.html`, `swarm/scripts/portal_full_page_audit.mjs`

## Root cause

Success toasts stayed visible for four seconds even after switching Portal views. A copy toast from Dashboard could still cover Providers, Models, Teams, or Usage screenshots. This was harmless functionally but weak UX: transient feedback from one surface should not follow the user into another surface.

## Fix applied

- Added `clearToasts()`.
- `go(view)` now clears existing toasts when the active view changes.
- Toast duration reduced from 4.0s to 3.2s.
- Toasts still remain visible when the user stays on the same page after copy/save/error actions.

## Regression gate

`portal_full_page_audit.mjs` now records visible toasts per page and fails if the fallback copy probe toast survives navigation beyond Dashboard.

## Verification

Commands run against the live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
python3 swarm/scripts/portal_static_gate.py
node --check swarm/scripts/portal_full_page_audit.mjs
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
```

Results:

- `PORTAL_STATIC_GATE PASS`
- full page Playwright audit: `PASS`, 0 failures, 0 console errors
- user journey Playwright audit: `PASS`, 0 failures, 0 console errors

Screenshot inspected manually:

- `swarm/out/playwright/20260918-073449-portal-full-page-audit/desktop-1440-providers.png`
