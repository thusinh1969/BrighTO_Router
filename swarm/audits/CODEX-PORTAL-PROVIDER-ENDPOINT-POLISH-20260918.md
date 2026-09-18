# CODEX — Portal Provider Endpoint Polish — 2026-09-18

Scope:

- `static/index.html`
- `swarm/scripts/portal_full_page_audit.mjs`
- `swarm/scripts/portal_polish_audit.mjs`
- `swarm/scripts/portal_user_journey_audit.mjs`

## Verdict

PASS for this polish slice. Keep the broader Portal SOTA goal open.

## What changed

1. Provider terminology is now clearer in the Portal:
   - User-facing text now says `Provider endpoint` instead of `Provider connection`.
   - The Providers page explains that endpoints are the base URLs BrighTO sends model traffic to.
   - The primary setup path remains `Add model`; manual endpoint setup is labelled `Advanced endpoint`.

2. Providers page layout was tightened:
   - Provider rows stay inside the panel on desktop.
   - Row actions use a compact 2-by-2 button grid instead of stretching across the row.
   - URL, route count, usage count, and action cells keep the card-row layout on desktop and mobile.

3. Dashboard/User chart wording is consistent:
   - Dashboard chart now says `Tokens by model (input + output) — last 30 days`.
   - First-run Dashboard now surfaces `Tokens/sec` as a metric admins will see after the first call.

4. API key owner/team/scope labels keep readable wrapping without clipping.

## Gates updated

`portal_full_page_audit.mjs` now verifies:

- Providers page uses `Advanced endpoint` as the optional setup action.
- Provider rows stay within the panel.
- Provider row actions remain a compact 2-by-2 grid.
- Provider rows keep card-level vertical space.

`portal_polish_audit.mjs` now verifies:

- Providers page uses `Advanced endpoint` for manual endpoint setup.
- Advanced endpoint modal creates/edits/deletes without stale duplicate panels.
- Dashboard uses current readiness/summary metrics instead of requiring removed generic cards.
- API key owner/team/scope labels wrap cleanly without clipping.

`portal_user_journey_audit.mjs` now verifies:

- User Dashboard chart explains that tokens are input + output.

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

- `swarm/out/playwright/20260918-085049-portal-full-page-audit/desktop-1440-providers.png`
- `swarm/out/playwright/20260918-085049-portal-full-page-audit/mobile-390-providers.png`
- `swarm/out/playwright/20260918-085157-portal-user-journey-audit/desktop-1440-user-dashboard.png`
