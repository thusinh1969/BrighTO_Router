# CODEX audit — Add model modal clarity and surface check

Date: 2026-09-17
Scope: Portal frontend polish for the model route creation/editing flow.

## Finding

The Add model modal still made two different names easy to confuse:

- Provider model: the exact upstream model name returned by OpenAI-compatible/Anthropic/etc. provider APIs.
- Public model name: the name client applications send to BrighTO Router.

The same modal also used the global sticky footer. On the long route form, Playwright proved the footer could cover the final fields on desktop.

## Fix applied

- Added help text under Provider model: `Exact upstream model name returned by the provider.`
- Added help text under Public model name: `The model name your apps send. Use the same name or a shorter team alias.`
- Disabled sticky actions only for `.route-modal` so long route forms do not hide inputs.
- Extended `portal_modal_surface_audit.mjs` so Add model screenshots must include both pieces of help text and must not have footer-covered inputs.

## Verification

Passed locally on the current worktree:

- `python3 swarm/scripts/portal_static_gate.py`
- JavaScript syntax extraction + `node --check /tmp/brighto-portal.js`
- `node --check` for portal Playwright audit scripts
- `git diff --check`
- `bash swarm/scripts/portal_empty_state_audit.sh`
- `bash swarm/scripts/portal_logic_acceptance.sh`
- `bash swarm/scripts/portal_visual_audit.sh`
- `bash swarm/scripts/portal_polish_audit.sh`
- `bash swarm/scripts/portal_full_page_audit.sh`
- `bash swarm/scripts/portal_modal_surface_audit.sh`
- `bash swarm/scripts/portal_user_journey_audit.sh`

Evidence logs:

- `swarm/out/portal_modal_surface_audit-route-actions-203949.log`
- `swarm/out/portal_empty_state_audit-model-help-full-204017.log`
- `swarm/out/portal_logic_acceptance-model-help-full-204022.log`
- `swarm/out/portal_visual_audit-model-help-full-204037.log`
- `swarm/out/portal_polish_audit-model-help-full-204046.log`
- `swarm/out/portal_full_page_audit-model-help-full-204055.log`
- `swarm/out/portal_modal_surface_audit-model-help-full-204114.log`
- `swarm/out/portal_user_journey_audit-model-help-full-204125.log`
