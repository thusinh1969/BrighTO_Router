# CODEX audit — Portal full-text data labels

Date: 2026-09-18
Role: Codex auditor / UI polish

## Verdict

PASS after fix.

Several Portal surfaces looked compact but were losing real data because labels were shortened in JavaScript before rendering. That is bad for admin work: screenshots look neat, but copied DOM text, visual audit checks, and user inspection no longer contain the full provider/model/team names.

## Fix applied

- `nameNode()` now renders the full value into the DOM and stores the same value in `title` / `aria-label`.
- CSS remains responsible for visual wrapping or ellipsis, so layout stays controlled without destroying the underlying text.
- Usage breakdown titles now wrap inside cards instead of single-line clipping.
- Full-page audit now fails if important data labels contain a JavaScript-created `…` truncation.

## Files changed

- `static/index.html`
- `swarm/scripts/portal_full_page_audit.mjs`

## Verification

Sequential gates against live HTTPS runtime:

- `portal_visual_audit.sh`: PASS
- `portal_full_page_audit.sh`: PASS
- `portal_polish_audit.sh`: PASS
- `portal_logic_acceptance.sh`: PASS
- `portal_modal_surface_audit.sh`: PASS
- `portal_login_audit.sh`: PASS

Key evidence:

- Dashboard, Usage, Models, Providers, Teams, API Keys all keep full data labels in DOM.
- Long names remain visually controlled by CSS without whole-page horizontal overflow.
- Usage breakdown cards are readable and no longer fail clipped-title checks.
