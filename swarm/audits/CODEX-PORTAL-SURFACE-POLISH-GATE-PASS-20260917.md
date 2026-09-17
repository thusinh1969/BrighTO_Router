# Codex Portal Surface Polish Gate Pass — 2026-09-17

Verdict: PASS for this frontend polish pass. Goal remains active for broader SOTA polish, but this round fixes concrete visible issues and verifies browser behavior.

Changed in `static/index.html`:

- Login is now a two-column product entry screen on desktop with a concise product overview, runtime facts, and operational proof points. Mobile keeps a focused single-card login.
- Model picker modal now has a real visible `Search models` label, explanatory header, accessible listbox rows, selected state, double-click selection, and stronger long-name wrapping.
- User dashboard now has a client workspace hero showing key prefix, team, enabled state, allowed models, requests, tokens, error rate, and average Tok/s.
- Removed duplicate DOM/code defects in Teams header and Models delete button declaration.
- Kept Settings visible for user because display preferences are useful and runtime details remain admin-only in render logic.

Verification:

- `node --check` on extracted portal script: PASS.
- Manual Playwright surface probe: PASS.
  - Desktop login screenshot captured.
  - Mobile login screenshot captured.
  - Model picker screenshot captured.
  - User dashboard screenshot captured.
- Repo gates: PASS.
  - `portal_empty_state_audit.sh`
  - `portal_logic_acceptance.sh`
  - `portal_visual_audit.sh`
  - `portal_polish_audit.sh`
  - `portal_full_page_audit.sh`

Notes:

- Current Docker compose serves live `static/index.html` through `PORTAL_STATIC_FILE=/app/static/index.html`, so local refresh sees UI changes without rebuilding Rust.
- Docker image still must be rebuilt and pushed so the official image embeds this frontend fallback inside the Rust binary.
