# CODEX audit — Provider naming and fallback route control

Date: 2026-09-17
Scope: Admin Portal provider/model route flow.

## Finding

The UI still mixed three terms for the same concept:

- Providers page title
- Connections panel/table
- Backend wording inside the model route fallback field

That was a real UX problem because the product already separates provider presets, provider connections, and model routes. The fallback field was worse: it exposed `Fallback backend (optional)` as a numeric input, forcing admins to know an internal database ID.

## Fix applied

- Standardized visible Admin UI copy around `Provider connections` / `provider endpoints`.
- Reworded provider page title description and table header.
- Removed visible `Fallback backend` wording from Add/Edit model.
- Replaced fallback numeric input with a `Fallback provider (optional)` dropdown listing existing provider connections.
- Kept the backend API payload unchanged: selected fallback still maps to `fallback_backend_id` internally.
- Updated `portal_modal_surface_audit.mjs` to fail if Add model exposes `Fallback backend` jargon again or does not expose fallback as provider selection.
- Updated `portal_polish_audit.mjs` to match the new `Provider connections` panel name.

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

- `swarm/out/portal_polish_audit-provider-naming-fallback-212504.log`
- `swarm/out/portal_full_page_audit-provider-naming-fallback-212513.log`
- `swarm/out/portal_modal_surface_audit-provider-naming-fallback-212532.log`
- `swarm/out/portal_login_audit-provider-naming-fallback-full-212612.log`
- `swarm/out/portal_empty_state_audit-provider-naming-fallback-full-212616.log`
- `swarm/out/portal_logic_acceptance-provider-naming-fallback-full-212621.log`
- `swarm/out/portal_visual_audit-provider-naming-fallback-full-212636.log`
- `swarm/out/portal_polish_audit-provider-naming-fallback-full-212644.log`
- `swarm/out/portal_full_page_audit-provider-naming-fallback-full-212653.log`
- `swarm/out/portal_modal_surface_audit-provider-naming-fallback-full-212712.log`
- `swarm/out/portal_user_journey_audit-provider-naming-fallback-full-212724.log`
