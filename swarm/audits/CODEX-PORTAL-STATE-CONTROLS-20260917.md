# CODEX audit — State controls in modals

Date: 2026-09-17
Scope: Portal modal UI polish for enable/disable state controls.

## Finding

Provider and team modals used the same `chip` styling as small filter pills for the `Enabled` checkbox. In modal forms this looked cramped and unclear, especially after the user had already called out checkbox-heavy UI as unprofessional.

## Fix applied

- Added a dedicated `switch-field` form control style for state toggles.
- Provider state now shows `Enabled` with helper text `Available for model routes.`
- Team state now shows `Enabled` with helper text `Team can own API keys and usage.`
- API key edit state now shows `Enabled` with helper text `Client key can call allowed routes.`
- Kept the underlying checkbox inputs and API payloads unchanged.
- Updated `portal_modal_surface_audit.mjs` to inspect computed style and fail if Add provider/New team state controls stop rendering as flex, left/right controls.

## Verification

Passed locally on the current worktree:

- `python3 swarm/scripts/portal_static_gate.py`
- JavaScript syntax extraction + `node --check /tmp/brighto-portal.js`
- `node --check` for all portal Playwright audit scripts
- `git diff --check`
- `bash swarm/scripts/portal_login_audit.sh`
- `bash swarm/scripts/portal_empty_state_audit.sh`
- `bash swarm/scripts/portal_logic_acceptance.sh`
- `bash swarm/scripts/portal_visual_audit.sh`
- `bash swarm/scripts/portal_polish_audit.sh`
- `bash swarm/scripts/portal_full_page_audit.sh`
- `bash swarm/scripts/portal_modal_surface_audit.sh`
- `bash swarm/scripts/portal_user_journey_audit.sh`

Evidence logs:

- `swarm/out/portal_modal_surface_audit-switch-field-210838.log`
- `swarm/out/portal_modal_surface_audit-switch-field-specificity-210907.log`
- `swarm/out/portal_modal_surface_audit-switch-regression-210951.log`
- `swarm/out/portal_login_audit-switch-field-full-211010.log`
- `swarm/out/portal_empty_state_audit-switch-field-full-211015.log`
- `swarm/out/portal_logic_acceptance-switch-field-full-211020.log`
- `swarm/out/portal_visual_audit-switch-field-full-211036.log`
- `swarm/out/portal_polish_audit-switch-field-full-211044.log`
- `swarm/out/portal_full_page_audit-switch-field-full-211052.log`
- `swarm/out/portal_modal_surface_audit-switch-field-full-211112.log`
- `swarm/out/portal_user_journey_audit-switch-field-full-211123.log`
