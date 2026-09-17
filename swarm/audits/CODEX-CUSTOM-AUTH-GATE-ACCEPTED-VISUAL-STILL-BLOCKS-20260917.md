# CODEX AUDIT — CUSTOM LLM AUTH ACCEPTED; VISUAL UI STILL BLOCKS V1.0 POLISH

Date: 2026-09-17  
Role: Codex auditor. Product implementation by DeepSeek; test gate hardening by Codex.

## Verdict

**Custom LLM blank-key and canonical wrapper gate are now accepted.**

**Portal V1.0 UI polish is still not accepted** because the Models table fails visual/responsive checks with long public model names.

## Fixed and verified

### Custom LLM blank-key semantics

DeepSeek commit:

- `cfe28ff fix: Custom LLM local blank key -> auth none + local_openai_chat (never silently apply env key to local URLs)`

Verified by canonical wrapper after Docker rebuild:

```bash
bash swarm/scripts/portal_logic_acceptance.sh
```

Result: PASS.

Key evidence from summary:

```json
{
  "auth_mode": "none",
  "protocol": "local_openai_chat"
}
```

This was tested while `.env CUSTOM_LLM_API_KEY` is set, with Admin leaving the wizard API key blank. That is the required llama.cpp/local behavior.

### Wrapper gate no longer false-greens this bug

Codex commit:

- `af6454d harden portal acceptance cleanup and connection selector`

Gate improvements:

- Docker Playwright path is used by default through `portal_logic_acceptance.sh`.
- Picker selection is scoped to the picker modal instead of global text.
- Custom LLM blank-key asserts exact route semantics: `auth_mode=none`, `protocol=local_openai_chat`.
- In-use connection lifecycle check scopes to base URL `127.0.0.1:9000/v1`, not the first row named `Custom LLM`.
- Cleanup removes `pw-*`, `crud-*`, `verify-*`, `mock-model`, related usage rows, and the 9000 mock backend.

### Rust verification

```bash
cargo check --workspace
cargo test --workspace
```

Result: PASS.

- 60 library tests pass.
- 4 streaming integration tests pass.
- 64 total tests pass.

## Current live DB after cleanup

- Backends: 1 (`Custom LLM`, `http://127.0.0.1:8088/v1`, no key, no active routes)
- Routes: 0
- Teams: 1 (`Default`)
- API keys: 1 demo key (`lc-01234...`, revealable)

## Remaining blocker: visual/responsive UI

The visual audit is still red:

- Audit: `swarm/audits/CODEX-VISUAL-RESPONSIVE-UI-FAIL-20260917.md`
- Artifact summary: `swarm/out/playwright/visual-responsive-20260917-172138/summary.json`
- Screenshots:
  - `swarm/out/playwright/visual-responsive-20260917-172138/desktop-models.png`
  - `swarm/out/playwright/visual-responsive-20260917-172138/laptop-models.png`

Hard failures:

1. Long public model names overlap status/provider/provider-model/action columns.
2. Public model cell has `overflow: visible`, `text-overflow: clip`, `white-space: nowrap`.
3. Action column is too narrow for Disable/Edit/Delete.
4. At 1024px, public model column collapses to ~75px while its text needs ~781px.
5. The result looks unprofessional and matches the user's complaint.

## Required next DeepSeek work

Do not spend more time on router logic now. Fix visual quality:

1. Models table must use explicit column widths or a route-card layout.
2. Public model and provider model cells must truncate cleanly with tooltip/copy.
3. Action column must reserve enough width for all actions.
4. Laptop width 1024px must not overlap text/buttons/badges.
5. Mobile must use hamburger correctly and present model rows as cards or a contained horizontal table.
6. Add a visual regression gate for long model names at 1440px, 1024px, and 390px.

Acceptance for UI polish requires the visual audit to pass, not just the logic gate.
