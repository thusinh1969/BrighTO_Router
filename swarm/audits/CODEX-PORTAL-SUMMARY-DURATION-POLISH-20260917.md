# Codex audit — Portal summary and duration polish — 2026-09-17

## What changed

The Portal now shows operational summary cards at the top of the main admin work tabs:

- Providers: total connections, active connections, locked connections, disabled connections.
- Models & Routes: live routes, drafts, provider-blocked routes, providers used.
- API Keys: issued keys, enabled keys, scoped keys, keys expiring in the next 7 days.

Provider row status was also made more professional:

- the pill is short (`Locked`, `Active`, `Disabled`, `Template`),
- the explanation is a smaller line below the pill,
- the pill no longer stretches across the table cell on desktop or mobile.

Request logs now render raw sub-millisecond router/duration timings as `<1 ms` instead of `0 ms`, while Tok/s still stays hidden as `—` for requests below 100 ms because that rate is not a useful production signal.

Settings now names browser-side display controls as `Portal preferences`.

## Why

The previous Providers/Models/API Keys pages were usable but looked sparse and forced the admin to scan tables for basic state. The summary strip gives the admin the page answer first, then the table details. The duration fix removes misleading `0 ms` values from logs.

## Permanent gates updated

- `swarm/scripts/portal_full_page_audit.mjs` now requires summary cards on Providers, Models, and API Keys for both desktop and mobile.
- `swarm/scripts/portal_polish_audit.mjs` now checks `fmtDur(0) != "0 ms"`, verifies Providers summary cards, and verifies Usage logs do not render `ROUTER 0 ms` or `DURATION 0 ms`.
- The Playwright console filter ignores only the known browser-level `ERR_NETWORK_CHANGED` noise; functional API and UI failures still fail the gates.

## Verification

Syntax/static:

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal script from `static/index.html` and ran `node --check /tmp/brighto-portal.js` — PASS
- `node --check swarm/scripts/portal_polish_audit.mjs` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `git diff --check` — PASS

Browser gates:

- `swarm/scripts/portal_empty_state_audit.sh` — PASS
- `swarm/scripts/portal_logic_acceptance.sh` — PASS
- `swarm/scripts/portal_visual_audit.sh` — PASS
- `swarm/scripts/portal_polish_audit.sh` — PASS
- `swarm/scripts/portal_full_page_audit.sh` — PASS
- `swarm/scripts/portal_modal_surface_audit.sh` — PASS

Representative logs:

- `swarm/out/portal_polish_audit-summary-strip-rerun-200426.log`
- `swarm/out/portal_polish_audit-summary-strip-final-200641.log`
- `swarm/out/portal_full_page_audit-summary-strip-final-200649.log`
- `swarm/out/portal_modal_surface_audit-summary-strip-final-200709.log`

Visual evidence:

- `swarm/out/playwright/20260917-200649-portal-full-page-audit/desktop-1440-providers.png`
- `swarm/out/playwright/20260917-200649-portal-full-page-audit/mobile-390-providers.png`
- `swarm/out/playwright/20260917-200649-portal-full-page-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260917-200649-portal-full-page-audit/desktop-1440-keys.png`
- `swarm/out/playwright/20260917-200649-portal-full-page-audit/desktop-1440-usage.png`
