# CODEX audit — Mobile Add Model actions visible

Date: 2026-09-18
Commit target: pending
Area: Portal UI, Add Model modal, mobile viewport

## Root cause fixed

The Add Model modal had the correct model-route logic, but on a 390px mobile viewport the primary actions (`Test connection`, `Save disabled`, `Save enabled`) were below the first viewport. A user could fill provider URL, API key, and model name, then not immediately see the required next action. This made the guided flow feel unfinished even when the logic worked.

## Change made

- Kept desktop modal behavior unchanged.
- On mobile only, made `.route-modal .actions` sticky at the bottom of the modal.
- Compacted the route modal form spacing and action bar so the footer remains visible without hiding the public model hint.
- Added a Playwright guard: mobile Add Model must keep the Test/Save action bar visible inside the viewport.

## Verification

Passed locally after the change:

- `portal_modal_surface_audit PASS swarm/out/portal_modal_surface_audit-mobile-route-compact-form-002713.log`
- `portal_polish_audit PASS swarm/out/portal_polish_audit-mobile-route-footer-final-002736.log`
- `portal_full_page_audit PASS swarm/out/portal_full_page_audit-mobile-route-footer-final-002736.log`
- `portal_user_journey_audit PASS swarm/out/portal_user_journey_audit-mobile-route-footer-final-002736.log`

## Remaining polish direction

Do not call the Portal SOTA yet. Continue with visual tightening of high-traffic admin flows: model/provider action density, chart readability on sparse data, and long provider/model names across desktop and mobile.
