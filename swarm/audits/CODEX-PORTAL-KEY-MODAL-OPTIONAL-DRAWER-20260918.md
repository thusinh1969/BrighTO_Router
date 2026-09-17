# CODEX audit — API key modal optional drawer

Date: 2026-09-18

## Verdict

Fixed API key creation flow density.

Creating a normal client API key should require only Team, Owner, and Model access. Before this change, the modal forced admins to scroll through expiry, request-rate limit, concurrency limit, token budget, period, token amount, and advanced budget rules before reaching Create. That made the default path feel heavier than it is.

## Change

- `static/index.html`
  - New API key modal now keeps limits and budget inside an `Optional limits and budget` drawer.
  - Create flow defaults to the drawer closed, so mobile shows Team, Owner, Scope, Optional drawer, Cancel, and Create in one viewport.
  - Edit flow opens the drawer automatically when the key already has expiry, rate/concurrency limit, or token budget.
  - Existing payload behavior is unchanged: expiry, RPM, concurrency, token budget, inherit team budget, and advanced budget rules are still supported.
- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Added gates that optional key fields are hidden by default and fully revealed after clicking the drawer.
- `swarm/scripts/portal_logic_acceptance.mjs`
  - Updated key budget edit flow to open the drawer only when the Budget field is not already visible.

## Verification

- `python3 swarm/scripts/portal_static_gate.py` — PASS
- extracted Portal JavaScript `node --check` — PASS
- `node --check swarm/scripts/portal_modal_surface_audit.mjs` — PASS
- `node --check swarm/scripts/portal_logic_acceptance.mjs` — PASS
- `node --check swarm/scripts/portal_full_page_audit.mjs` — PASS
- `git diff --check` — PASS
- `bash swarm/scripts/portal_modal_surface_audit.sh` — PASS
  - Log: `swarm/out/portal_modal_surface_audit-key-modal-optional-drawer-final2-000331.log`
- `bash swarm/scripts/portal_logic_acceptance.sh` — PASS
  - Log: `swarm/out/portal_logic_acceptance-key-modal-optional-drawer-final2-000331.log`
- `bash swarm/scripts/portal_full_page_audit.sh` — PASS
  - Log: `swarm/out/portal_full_page_audit-key-modal-optional-drawer-final2-000331.log`

## Screenshot evidence

- `swarm/out/playwright/20260918-000025-portal-modal-surface-audit/mobile-new-key.png`
