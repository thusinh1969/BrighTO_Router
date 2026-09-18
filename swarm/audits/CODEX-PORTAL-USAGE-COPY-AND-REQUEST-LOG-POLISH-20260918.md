# CODEX — Portal Usage Copy and Request Log Polish — 2026-09-18

Scope:

- `static/index.html`
- `swarm/scripts/portal_full_page_audit.mjs`
- `swarm/scripts/portal_user_journey_audit.mjs`

## Verdict

PASS for this polish slice. Keep the broader Portal SOTA goal open.

## What changed

1. Usage page copy now states the production meaning directly:
   - Topbar: `Tokens, cost, errors, and speed`
   - Chart: `Tokens by model (input + output)`
   - Breakdowns: `Compare token volume by model, team, and API key for this range.`

2. Request logs now show the team name instead of a raw numeric team id.

3. Request log KPI values wrap instead of clipping with ellipsis, so long team names and key prefixes remain readable on desktop and mobile.

## Gates added

`portal_full_page_audit.mjs` now fails if:

- Admin Usage topbar does not explain tokens/cost/errors/speed.
- Usage chart title does not explain input + output token totals.
- Usage breakdown copy does not explain model/team/API-key comparison.
- Dashboard or Usage request-log KPI values clip instead of wrapping.
- Admin request logs show raw numeric team ids.

`portal_user_journey_audit.mjs` now fails if:

- User Usage topbar is not role-aware.
- User Usage misses own model, Tokens/sec, or the input+output chart meaning.

## Verification

Commands run against the live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
python3 swarm/scripts/portal_static_gate.py
node --check swarm/scripts/portal_full_page_audit.mjs
node --check swarm/scripts/portal_user_journey_audit.mjs
node --check swarm/scripts/portal_modal_surface_audit.mjs
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
```

Results:

- `portal_static_gate.py`: PASS
- `portal_full_page_audit.sh`: PASS, 0 failures, 0 console errors
- `portal_user_journey_audit.sh`: PASS, 0 failures, 0 console errors
- `portal_modal_surface_audit.sh`: PASS, 0 failures, 0 console errors

Visual evidence:

- `swarm/out/playwright/20260918-083625-portal-full-page-audit/desktop-1440-usage.png`
- `swarm/out/playwright/20260918-083625-portal-full-page-audit/mobile-390-usage.png`
