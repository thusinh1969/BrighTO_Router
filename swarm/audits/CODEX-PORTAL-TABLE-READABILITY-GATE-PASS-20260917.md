# Codex Portal Table Readability Gate Pass — 2026-09-17

Verdict: PASS for this table readability polish pass.

Root cause fixed:

- Providers and Models tables were technically responsive, but long provider/model names were still too aggressively clipped on desktop.
- Route settings were dense inline text, which made Models harder to scan.
- Provider row actions stacked as a plain vertical list when the action column was narrow.

Changed in `static/index.html`:

- Provider and provider-model cells now wrap to two clean lines before truncating.
- Provider table column balance gives connection names more readable space.
- Models table column balance gives provider names more space while keeping action and settings columns bounded.
- Route settings now render as compact chips, making auth/context/output/pricing easier to scan.
- Provider action buttons now use a 2x2 grid on desktop-sized table rows.

Verification:

- `git diff --check`: PASS.
- Extracted Portal JS `node --check`: PASS.
- Full Portal gate suite: PASS.
  - `portal_empty_state_audit.sh`
  - `portal_logic_acceptance.sh`
  - `portal_visual_audit.sh`
  - `portal_polish_audit.sh`
  - `portal_full_page_audit.sh`

Manual visual evidence:

- Desktop Models screenshot shows provider name and route-setting chips without overflow.
- Desktop Providers screenshot shows full connection name and balanced 2x2 actions.
- Mobile Models screenshot shows route-setting chips and long names remain readable without horizontal overflow.
