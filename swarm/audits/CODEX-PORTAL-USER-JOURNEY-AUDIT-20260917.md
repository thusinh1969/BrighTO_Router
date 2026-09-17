# CODEX audit — User journey screenshot correctness and call endpoint readability

Date: 2026-09-17
Scope: User Portal dashboard and Playwright evidence quality.

## Finding

The user journey audit could pass while the desktop dashboard screenshot was visually blank. The DOM evidence showed content existed, but the screenshot was taken immediately after login/topbar readiness, before async dashboard content had painted. That made the visual evidence weaker than the DOM checks.

A second visual issue was visible after fixing the wait: the Call endpoint `AUTH` code block used nowrap/ellipsis, so `Authorization: Bearer <your API key>` could be visually clipped on desktop.

## Fix applied

- `portal_user_journey_audit.mjs` now waits for expected content per user view before taking screenshots.
- The wait is case-insensitive so labels such as `MODEL SCOPE` do not break the gate.
- User journey audit now records call endpoint code metrics and fails if endpoint/auth code is clipped or rendered with nowrap/ellipsis.
- `.call-item code` now wraps on all viewports so method, base URL, endpoint path, auth header, and model scope remain visible.

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

- `swarm/out/portal_user_journey_audit-wait-content-211653.log` — reproduced the wait bug.
- `swarm/out/portal_user_journey_audit-wait-content-rerun-211727.log` — fixed wait and non-blank dashboard screenshot.
- `swarm/out/portal_user_journey_audit-call-code-wrap-211802.log` — fixed call endpoint code clipping.
- `swarm/out/portal_login_audit-user-screenshot-call-wrap-full-211830.log`
- `swarm/out/portal_empty_state_audit-user-screenshot-call-wrap-full-211835.log`
- `swarm/out/portal_logic_acceptance-user-screenshot-call-wrap-full-211839.log`
- `swarm/out/portal_visual_audit-user-screenshot-call-wrap-full-211854.log`
- `swarm/out/portal_polish_audit-user-screenshot-call-wrap-full-211903.log`
- `swarm/out/portal_full_page_audit-user-screenshot-call-wrap-full-211912.log`
- `swarm/out/portal_modal_surface_audit-user-screenshot-call-wrap-full-211931.log`
- `swarm/out/portal_user_journey_audit-user-screenshot-call-wrap-full-211943.log`
