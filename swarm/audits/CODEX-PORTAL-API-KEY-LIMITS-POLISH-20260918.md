# CODEX — Portal API Key Limits Polish — 2026-09-18

Scope:

- `static/index.html`
- `swarm/scripts/portal_full_page_audit.mjs`
- `swarm/scripts/portal_polish_audit.mjs`

## Verdict

PASS for this polish slice. Keep the broader Portal SOTA goal open.

## What changed

1. API Keys topbar now explains the Admin task directly: client access, expiry, budgets, and model scope.

2. API key row semantics were corrected:
   - `Scope` now shows only model access scope.
   - Expiry moved from `Scope` into `Limits`.
   - `Limits` now contains expiry, requests/min, simultaneous calls, budget, and enabled state.

3. Desktop API key row layout gives more width to the Limits card because it carries more policy information.

## Gates added

`portal_full_page_audit.mjs` now fails if:

- Expiry appears inside the Scope cell.
- Any key Limits cell does not include an Expiry policy.

`portal_polish_audit.mjs` now checks the same Scope-vs-Limits invariant during the real Admin flow after creating usage data.

## Verification before release

Commands run against live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
python3 swarm/scripts/portal_static_gate.py
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
```

Results:

- `portal_static_gate.py`: PASS
- `portal_full_page_audit.sh`: PASS, 0 failures, 0 console errors
- `portal_polish_audit.sh`: PASS, 0 failures, 0 bugs, 0 console errors
- `portal_user_journey_audit.sh`: PASS, 0 failures, 0 console errors
- `portal_modal_surface_audit.sh`: PASS, 0 failures, 0 console errors

Visual evidence:

- `swarm/out/playwright/20260918-085841-portal-full-page-audit/desktop-1440-keys.png`
- `swarm/out/playwright/20260918-085841-portal-full-page-audit/mobile-390-keys.png`
