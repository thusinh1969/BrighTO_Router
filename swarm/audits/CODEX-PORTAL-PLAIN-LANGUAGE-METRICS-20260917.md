# CODEX audit — Plain-language metrics labels

Date: 2026-09-17
Scope: Admin/User Portal metric labels and dashboards.

## Finding

Several visible labels still used terse engineering shorthand:

- `Tok/s`
- `429`
- `5xx`
- `Ctx`
- `Out`
- `$/In`, `$/Out`
- `RPM`
- `Conc.`
- `Max concurrent`
- `First-byte timeout`

These are useful concepts, but the Portal should read clearly for SME/team admins without requiring them to decode shorthand.

## Fix applied

- Changed `Tok/s` to `Tokens/sec` in dashboard, usage logs, and user summary.
- Changed provider health headers from `429` / `5xx` to `Rate limits` / `Provider errors`.
- Added a p95 legend in Performance diagnostics: p95 means 95% of requests were this fast or faster; first byte means the wait until the provider starts responding.
- Changed model route setting chips from `Ctx`, `Out`, `$/In`, `$/Out` to `Context`, `Max output`, `Input $/1M`, `Output $/1M`.
- Changed API key limit labels from `RPM` / `Conc.` to `Requests/min` / `Simultaneous`.
- Changed provider modal label from `Max concurrent` to `Simultaneous calls`.
- Changed API key modal label from `Concurrency limit` to `Simultaneous request limit`.
- Changed route modal label from `First-byte timeout` to `Provider start timeout`, with helper text explaining it.
- Updated Playwright gates and selfcheck scripts for the new visible labels.
- Updated modal audit to ignore known transient `ERR_NETWORK_CHANGED`, matching the other portal gates.

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

- `swarm/out/portal_visual_audit-plain-labels-213924.log`
- `swarm/out/portal_polish_audit-plain-labels-rerun-213954.log`
- `swarm/out/portal_full_page_audit-plain-labels-rerun-214011.log`
- `swarm/out/portal_user_journey_audit-plain-labels-rerun-214031.log`
- `swarm/out/portal_login_audit-plain-language-labels-full2-214316.log`
- `swarm/out/portal_empty_state_audit-plain-language-labels-full2-214321.log`
- `swarm/out/portal_logic_acceptance-plain-language-labels-full2-214326.log`
- `swarm/out/portal_visual_audit-plain-language-labels-full2-214342.log`
- `swarm/out/portal_polish_audit-plain-language-labels-full2-214350.log`
- `swarm/out/portal_full_page_audit-plain-language-labels-full2-214359.log`
- `swarm/out/portal_modal_surface_audit-plain-language-labels-full2-214418.log`
- `swarm/out/portal_user_journey_audit-plain-language-labels-full2-214430.log`
