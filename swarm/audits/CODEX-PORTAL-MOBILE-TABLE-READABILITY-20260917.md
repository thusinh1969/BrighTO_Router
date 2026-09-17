# CODEX audit — Mobile table readability

Date: 2026-09-17
Scope: Portal mobile UI polish for admin tables and usage breakdowns.

## Finding

The mobile table-card layout passed overflow checks, but long values were still visually weak because cells used a two-column layout: label on the left, value right-aligned on the right. Long model names, provider names, and API keys wrapped awkwardly and looked harder to scan on 390px screens.

Affected screenshots before the fix:

- `mobile-390-models.png`
- `mobile-390-keys.png`
- `mobile-390-usage.png`

## Fix applied

- Changed mobile table cells to a stacked layout: label above, value below.
- Made mobile table values left-aligned.
- Kept desktop tables unchanged.
- Updated `portal_full_page_audit.mjs` to assert that mobile table cells render as `display: block`, text-align left, with visible block labels from `::before`.

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

- `swarm/out/portal_full_page_audit-mobile-table-stack-205226.log`
- `swarm/out/portal_modal_surface_audit-mobile-stack-rerun-205351.log`
- `swarm/out/portal_user_journey_audit-mobile-stack-205408.log`
- `swarm/out/portal_full_page_audit-mobile-cell-layout-205446.log`
- `swarm/out/portal_empty_state_audit-mobile-cell-full-205512.log`
- `swarm/out/portal_logic_acceptance-mobile-cell-full-205517.log`
- `swarm/out/portal_visual_audit-mobile-cell-full-205532.log`
- `swarm/out/portal_polish_audit-mobile-cell-full-205540.log`
- `swarm/out/portal_full_page_audit-mobile-cell-full-205549.log`
- `swarm/out/portal_modal_surface_audit-mobile-cell-full-205608.log`
- `swarm/out/portal_user_journey_audit-mobile-cell-full-205620.log`

Notes:

- One `portal_modal_surface_audit` run timed out while waiting for login. The app was healthy and serving HTML; immediate rerun passed. No CSS/layout failure was observed in that failed run.
