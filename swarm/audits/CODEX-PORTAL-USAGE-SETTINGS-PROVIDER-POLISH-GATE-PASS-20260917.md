# CODEX audit — Portal Usage/Settings/Provider polish gate PASS — 2026-09-17

## Verdict

PASS for this polish round. Usage, Settings, and Provider connection setup are now more consistent with the rest of the Portal.

This is still progress toward the larger SOTA Portal goal, not a final completion claim for the whole product.

## What changed

1. Usage filters now use a dedicated responsive filter grid instead of a loose wrapped row.
2. Usage added:
   - explanatory filter copy
   - 7d / 30d / 90d quick range buttons
   - Reset filters action
   - full, non-truncated select labels on desktop
3. Settings was changed from two plain panels into card-based sections:
   - Display preferences
   - Runtime status
   - status tiles for router address, database, config reload, max body bytes, version
4. Provider connection modal now explains the product logic clearly:
   - normal users should create models from Models & Routes → Add model
   - Provider modal edits/registers an upstream connection only
   - provider API keys live on model routes, not on provider connections
5. Provider modal fields are grouped into:
   - Connection preset
   - Endpoint
   - Load control
   - State
6. No backend API or routing behavior was changed.

## Verification

Clean gate run passed:

- `node --check` on the extracted Portal JavaScript
- `swarm/scripts/portal_empty_state_audit.sh`
- `swarm/scripts/portal_logic_acceptance.sh`
- `swarm/scripts/portal_visual_audit.sh`
- `swarm/scripts/portal_polish_audit.sh`
- `swarm/scripts/portal_full_page_audit.sh`

Additional visual review was done on Usage, Settings, and Provider modal screenshots.
