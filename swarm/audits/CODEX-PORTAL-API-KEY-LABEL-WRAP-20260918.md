# CODEX audit — API key owner and scope label wrapping

Date: 2026-09-18
Scope: Portal `API Keys` page.

## Finding
Visual review found that long owner/team labels in API Keys were still visually truncated by the generic `.compact-line` CSS rule. The existing audit did not catch CSS ellipsis because the underlying text was still present in the DOM.

## Fix applied
- Added a stronger `table.key-list-table ...` override so Owner/Team and Scope labels wrap instead of hiding behind CSS ellipsis.
- Added Playwright evidence collection for key owner/team/scope label computed styles and dimensions.
- Added a full-page audit failure if those labels use `nowrap`, `ellipsis`, or have hidden overflow.

## Evidence
- `BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh` passed with 0 failures and 0 console errors after the gate was added.
- `python3 swarm/scripts/portal_static_gate.py` passed.
- Visual screenshot inspected: `swarm/out/playwright/20260918-062657-portal-full-page-audit/desktop-1440-keys.png`.
