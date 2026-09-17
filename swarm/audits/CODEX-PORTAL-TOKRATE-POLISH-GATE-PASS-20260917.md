# Codex Portal Tok/s Polish Gate Pass — 2026-09-17

Verdict: PASS for the Tok/s display correction.

Root cause fixed:

- The Portal previously rendered compact Tok/s whenever backend returned `total_tokens_per_second`.
- Mock requests can complete with `total_ms = 0`, which produces huge calculated rates like `89K Tok/s` from a tiny functional smoke call.
- That value is mathematically derived but not a useful production speed signal, and it makes the UI look like it is reporting fake benchmark numbers.

Changed:

- `static/index.html` now renders Tok/s only when `total_ms >= 100` and the rate is finite.
- For sub-100ms calls, Tok/s renders as `—` with a tooltip explaining that duration is too short to be meaningful.
- User dashboard average Tok/s now ignores sub-100ms rows.
- Usage page hint now explains this rule.
- `swarm/scripts/portal_polish_audit.mjs` now verifies the corrected behavior instead of requiring `89K` for a `0 ms` mock request.

Verification:

- Extracted Portal JS `node --check`: PASS.
- `portal_polish_audit.sh`: PASS.
- `portal_full_page_audit.sh`: PASS.
- Full Portal gate suite: PASS.
  - `portal_empty_state_audit.sh`
  - `portal_logic_acceptance.sh`
  - `portal_visual_audit.sh`
  - `portal_polish_audit.sh`
  - `portal_full_page_audit.sh`

Evidence:

- Latest full-page Usage screenshot shows Request logs with `TOK/S —` and `DURATION 0 ms`, which is the intended production-safe display for mock/too-fast rows.
