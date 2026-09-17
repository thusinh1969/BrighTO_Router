# CODEX audit — Model route save label clarity

Date: 2026-09-17
Scope: Add/Edit model route modal.

## Finding

The model route modal used `Save draft` for the disabled-save path. In edit mode this was ambiguous: an admin editing an enabled route could click it without realizing the route would be saved disabled.

## Fix applied

- Renamed the action to `Save disabled`.
- Updated the button title to say it saves the route disabled and does not require a connection test.
- Updated the disabled-save toast to `Model saved (disabled)`.
- Updated logic/selfcheck scripts to use the new explicit button label.
- Extended modal surface audit to fail if `Save draft` returns in the Add model modal.

## Verification

Passed locally on the current worktree:

- `python3 swarm/scripts/portal_static_gate.py`
- JavaScript syntax extraction + `node --check /tmp/brighto-portal.js`
- `node --check` for all portal Playwright audit scripts plus selfcheck scripts
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

- `swarm/out/portal_login_audit-provider-fallback-save-disabled-full-213305.log`
- `swarm/out/portal_empty_state_audit-provider-fallback-save-disabled-full-213310.log`
- `swarm/out/portal_logic_acceptance-provider-fallback-save-disabled-full-213315.log`
- `swarm/out/portal_visual_audit-provider-fallback-save-disabled-full-213330.log`
- `swarm/out/portal_polish_audit-provider-fallback-save-disabled-full-213339.log`
- `swarm/out/portal_full_page_audit-provider-fallback-save-disabled-full-213347.log`
- `swarm/out/portal_modal_surface_audit-provider-fallback-save-disabled-full-213406.log`
- `swarm/out/portal_user_journey_audit-provider-fallback-save-disabled-full-213418.log`
