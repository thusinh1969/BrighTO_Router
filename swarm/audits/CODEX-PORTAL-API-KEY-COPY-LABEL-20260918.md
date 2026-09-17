# CODEX audit — Portal API key copy action

Date: 2026-09-18
Role: Codex auditor / UI polish

## Verdict

PASS after fix.

API Keys showed the full key, but the copy action was only an unlabeled icon. After adding a text label, the first attempt overlapped the key text. The final layout keeps the full key readable on its own line and shows an explicit `Copy key` button below it.

## Fix applied

- API key rows now use a labelled `Copy key` button.
- The button keeps the existing `icon-btn` class so existing copy-action audits still recognize it.
- Key secret layout is now a two-row grid: full key first, copy action second.
- Client-key column width was increased from 37% to 43% so full keys have priority over owner/scope metadata.
- Logic and full-page audits now require an explicitly labelled copy action for visible API keys.

## Files changed

- `static/index.html`
- `swarm/scripts/portal_full_page_audit.mjs`
- `swarm/scripts/portal_logic_acceptance.mjs`

## Verification

Sequential gates against live HTTPS runtime:

- `portal_visual_audit.sh`: PASS
- `portal_full_page_audit.sh`: PASS
- `portal_polish_audit.sh`: PASS
- `portal_logic_acceptance.sh`: PASS
- `portal_modal_surface_audit.sh`: PASS
- `portal_login_audit.sh`: PASS

Visual check:

- Desktop API Keys screenshot shows full key, clear `Copy key` action, and no overlap.
