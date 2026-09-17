# Codex Portal Modal Surface Gate Pass — 2026-09-17

Verdict: PASS for modal form polish and modal regression coverage.

Root causes fixed:

- Add Model desktop modal was taller than the visible modal area and the sticky footer could cover the last inputs.
- On mobile, sticky modal footers could cover inputs in long forms such as New API key.
- Provider model input and Load models button were squeezed into one row on mobile.
- Existing full-page gates did not capture modal surfaces, so these bugs could regress without failing CI-style browser checks.

Changed in `static/index.html`:

- Added modal-specific hint and field spacing to reduce unnecessary form height.
- Changed Add Model's model-name section to a two-column layout on desktop while keeping one column on mobile.
- Disabled sticky modal footer behavior on mobile so action buttons stay in normal scroll flow and never cover form inputs.
- Stacked Provider model input and Load models button on mobile for readable input width.

Added permanent browser gate:

- `swarm/scripts/portal_modal_surface_audit.sh`
- `swarm/scripts/portal_modal_surface_audit.mjs`

The new gate logs in as Admin, opens these modals in desktop and mobile viewports, captures screenshots, and fails on missing modals, clipped content, console errors, or footer-covered input fields:

- Add model
- Add provider
- New team
- New API key

Verification:

- `git diff --check`: PASS.
- Extracted Portal JS `node --check`: PASS.
- Modal audit JS `node --check`: PASS.
- Full browser gate suite: PASS.
  - `portal_empty_state_audit.sh`
  - `portal_logic_acceptance.sh`
  - `portal_visual_audit.sh`
  - `portal_polish_audit.sh`
  - `portal_full_page_audit.sh`
  - `portal_modal_surface_audit.sh`

Manual visual evidence:

- Desktop Add Model screenshot shows all bottom fields visible above the footer.
- Mobile Add Model screenshot shows Provider model input full width and Load models button below it.
- Mobile New API key modal no longer has footer-covered Budget field.
