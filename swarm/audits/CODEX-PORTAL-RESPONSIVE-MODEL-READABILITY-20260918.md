# CODEX audit — Portal responsive model readability

Date: 2026-09-18
Role: Codex auditor / UI polish

## Verdict

PASS after fix.

The Models & Routes screen had a real responsive readability gap: long public model names were shortened by JavaScript and the 1024px layout kept the fixed sidebar, leaving the model table cramped. This made the most important value on the page hard to read and previously caused visual audit failure because the full route name was not present in the rendered text.

## Fix applied

- Public model names in the Models table now render as the full text and wrap inside the route card.
- Copy button remains in a stable separate grid column beside the model name.
- Desktop route-card grid no longer uses oversized hard minimum widths that spill out at laptop size.
- Sidebar collapses to an overlay below 1100px, giving tablet/laptop screens full content width.
- Topbar now groups hamburger + page title on the left, so the heading stays left-aligned when sidebar is collapsed.

## Files changed

- `static/index.html`

## Verification

Sequential gates against live HTTPS runtime:

- `portal_visual_audit.sh`: PASS
- `portal_full_page_audit.sh`: PASS
- `portal_polish_audit.sh`: PASS
- `portal_logic_acceptance.sh`: PASS
- `portal_modal_surface_audit.sh`: PASS
- `portal_login_audit.sh`: PASS

Important evidence:

- 1440px, 1024px, and 390px Models views have no whole-page horizontal overflow.
- Long public model name is present in rendered text, not destroyed by JS truncation.
- Public model/provider/provider-model/action columns stay controlled.
- 1024px screen uses overlay navigation and a full-width content panel.
