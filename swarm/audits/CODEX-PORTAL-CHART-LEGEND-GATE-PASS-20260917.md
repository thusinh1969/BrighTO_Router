# Codex Portal Chart Legend Gate Pass — 2026-09-17

Verdict: PASS for chart legend readability.

Root cause fixed:

- Mobile Usage charts could silently clip long model names in the legend.
- Existing full-page audit caught page/table overflow but did not catch hidden legend text.

Changed:

- `static/index.html` legend labels now wrap with `overflow-wrap:anywhere` and keep the color marker aligned.
- `swarm/scripts/portal_full_page_audit.mjs` now records legend dimensions and fails if chart legend text is clipped.

Verification:

- Extracted Portal JS `node --check`: PASS.
- `swarm/scripts/portal_full_page_audit.mjs` `node --check`: PASS.
- `git diff --check`: PASS.
- Full Portal gate suite: PASS.
  - `portal_empty_state_audit.sh`
  - `portal_logic_acceptance.sh`
  - `portal_visual_audit.sh`
  - `portal_polish_audit.sh`
  - `portal_full_page_audit.sh`

Manual visual evidence:

- Latest mobile Usage screenshot shows the long model legend wrapped instead of being hidden at the card edge.
