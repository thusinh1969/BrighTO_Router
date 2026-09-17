# CODEX audit — Mobile Add Model modal action bar

Date: 2026-09-18
Role: Codex auditor / UI polish

## Verdict

PASS after fix.

The mobile Add Model modal kept the Test/Save actions visible, but the action bar was too tall and used a negative sticky offset. In screenshots it visually covered lower form content, making the flow feel cramped.

## Fix applied

- Mobile Add Model action bar now sticks to the bottom with a safer `bottom: 0` geometry.
- The footer is compacted into a four-column grid so it consumes less vertical space.
- Mobile button sizing was reduced for this modal only.
- The public-to-provider model mapping preview remains present and compact on mobile.
- The modal keeps enough bottom padding so the sticky action bar does not cover input fields.

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

Visual check:

- Mobile Add Model screenshot shows provider/model inputs usable with Test/Save actions visible.
