# Codex audit — API key budget edit gate pass — 2026-09-17

## Root cause fixed

Editing an API key that already had a budget left the hidden Advanced JSON textarea populated. When the admin changed the Budget selector back to `Inherit team budget`, the save path still parsed that hidden JSON first, so the old key budget could survive instead of being cleared.

## Fix

`static/index.html` now treats `Budget = Inherit team budget` as an explicit `budget: null` before checking the advanced JSON textarea. Advanced JSON is only read when it is visible and the key is not inheriting the team budget.

## Permanent gate added

`swarm/scripts/portal_logic_acceptance.mjs` now verifies the full edit path:

1. create an API key,
2. edit it to set a token budget,
3. verify `/admin/keys` reports `max_tokens = 1234`,
4. edit it back to inherit the team budget,
5. verify `/admin/keys` reports `budget = null`,
6. continue reveal / disable / enable checks.

## Verification

Syntax:

- `node --check swarm/scripts/portal_logic_acceptance.mjs`
- extracted Portal script from `static/index.html` and ran `node --check /tmp/brighto-portal.js`
- `git diff --check`

Browser gates:

- `swarm/scripts/portal_empty_state_audit.sh` — PASS
- `swarm/scripts/portal_logic_acceptance.sh` — PASS
- `swarm/scripts/portal_visual_audit.sh` — PASS
- `swarm/scripts/portal_polish_audit.sh` — PASS
- `swarm/scripts/portal_full_page_audit.sh` — PASS
- `swarm/scripts/portal_modal_surface_audit.sh` — PASS

Representative logs:

- `swarm/out/portal_logic_acceptance-key-budget-195046.log`
- `swarm/out/portal_logic_acceptance-rerun-195149.log`
- `swarm/out/portal_logic_acceptance-key-budget-full2-195212.log`
- `swarm/out/portal_modal_surface_audit-key-budget-full2-195304.log`
