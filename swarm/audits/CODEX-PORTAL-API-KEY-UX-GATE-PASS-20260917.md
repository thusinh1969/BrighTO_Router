# Codex Portal API Key UX Gate Pass — 2026-09-17

Verdict: PASS for API key usability and key-state correctness.

Root causes fixed:

- API Keys table revealed key text and copy button, but once reveal succeeded there was no explicit button to open the full-key modal again.
- The disabled-key row button could say `Enable`, but the click handler still called the disable endpoint. The UI could not re-enable a disabled key from the table.
- The key-created modal used plain text for a hint containing `<b>API Keys</b>`, so the markup could display literally instead of as emphasis.
- Add Model and long mobile modals had footer/input overlap risks that were not covered by the existing page-level gates.

Changed:

- API Keys table now shows a compact `Reveal` action after key reveal succeeds, alongside copy.
- Key state action now uses `toggleKey`: enabled keys call DELETE to disable; disabled keys call PATCH `{enabled:true}` to re-enable.
- Key-created modal hint renders the `API Keys` emphasis as HTML.
- Removed duplicate team-id assignment in the API key modal setup.
- Add Model modal is more compact and avoids footer-covered fields.
- Mobile modal actions are normal-flow, not sticky, so buttons cannot cover inputs while scrolling.
- Added `portal_modal_surface_audit` to permanently test Add model, Add provider, New team, and New API key modals on desktop and mobile.
- Updated browser gates with retry around transient Playwright `page.goto` network errors.

Verification:

- `git diff --check`: PASS.
- Extracted Portal JS `node --check`: PASS.
- Browser gate JS syntax checks: PASS.
- Full browser gate suite: PASS.
  - `portal_empty_state_audit.sh`
  - `portal_logic_acceptance.sh`
  - `portal_visual_audit.sh`
  - `portal_polish_audit.sh`
  - `portal_full_page_audit.sh`
  - `portal_modal_surface_audit.sh`

Acceptance now covers:

- Created API key row exposes `Reveal`.
- `Reveal` opens the full-key modal.
- `Disable` makes the key disabled in the API.
- `Enable` re-enables that same key through PATCH.
- Modal surfaces have no clipped content and no footer-covered inputs.
