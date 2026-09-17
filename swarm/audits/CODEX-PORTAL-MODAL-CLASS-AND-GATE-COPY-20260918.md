# CODEX audit — Modal class separation and Add Model gate copy

Date: 2026-09-18
Commit target: pending
Area: Portal modal UX, Add Model and API Keys

## Root cause fixed

`openKeyModal()` used the `route-modal` class. That class is intended for the Add Model wizard and makes mobile actions sticky. As a result, the expanded API Key limits form could have its footer overlap fields on mobile. The issue was exposed after adding a stricter modal-surface audit.

Add Model also had the correct Save-enabled gating, but the reason was not visible early enough on mobile. The text existed in the DOM but sat too close to the sticky footer.

## Change made

- Added `wide-modal` for wide non-route modals.
- Restored `route-modal` only for Add/Edit Model.
- Changed API Key modal to `wide-modal`, so its footer is static on mobile.
- Moved `Test connection to enable Save enabled.` directly under the `Model` section heading.
- Modal audit now verifies Add Model gate copy is visible above sticky mobile actions, not just present in the DOM.

## Verification

Passed locally after the change:

- `portal_modal_surface_audit PASS swarm/out/portal_modal_surface_audit-model-gate-copy-above-fields-010121.log`
- `portal_full_page_audit PASS swarm/out/portal_full_page_audit-modal-class-gate-final-010212.log`
- `portal_logic_acceptance PASS swarm/out/portal_logic_acceptance-modal-class-gate-final-010212.log`
- `portal_polish_audit PASS swarm/out/portal_polish_audit-modal-class-gate-final-010212.log`

## Current evidence

- Add Model mobile gate copy top/bottom: 480/497, safely above sticky footer top 748.
- API Key expanded mobile footer is static after class separation.
