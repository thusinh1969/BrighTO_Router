# CODEX audit — first-run Dashboard onboarding tone — 2026-09-18

## Problem

With a clean database, the Admin Dashboard had no providers and no routes, but the hero still said `Needs attention` and the cost KPI said `priced routes ready`.

That is technically understandable from generic health logic, but wrong for a first-time install. A new admin should see a setup state, not an alarm state or a false pricing-ready claim.

## Fix

Changed `static/index.html` Dashboard hero logic:

- Detects first-run onboarding when `routes.length === 0`.
- Hero title becomes `Ready to set up`.
- Hero copy explains the next action: add one tested model route; BrighTO creates the provider connection.
- Cost KPI becomes `— / add route pricing` instead of `$0.00 / priced routes ready`.
- Error KPI becomes `0% / no traffic yet`.
- Readiness copy changes from raw `0/0` failure wording to setup tasks:
  - Routes: `add first model`
  - Provider connections: `created with model`
  - Pricing: `set per route later`

Updated `swarm/scripts/portal_empty_state_audit.mjs`:

- Fails if first-run Dashboard says `Needs attention`.
- Fails if first-run Dashboard claims `priced routes ready` before any route exists.
- Fails if first-run readiness copy does not explain setup actions.

## Verified

Commands run against live HTTPS portal:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_empty_state_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
```

Result: all PASS.

Visual evidence:

- `swarm/out/playwright/20260918-021330-portal-empty-state-audit/empty-dashboard.png`

## Verdict

The clean-install Portal now presents a professional onboarding state. It no longer makes a new installation look broken or claims pricing is ready before a route exists.
