# CODEX Portal Add Model coming-soon provider guard — 2026-09-17

## Result

PASS. Add Model no longer presents catalog entries marked `enabled:false` as normal selectable providers.

## Problem fixed

The provider catalog already marks Gemini and Meta Muse as disabled/coming-soon, but the Add Model dropdown rendered every catalog entry as a normal option. That made unsupported providers look ready and could lead admins into confusing connection-test failures.

## Change

- `static/index.html`
  - Add Model provider dropdown now labels disabled catalog entries as `(coming soon)`.
  - Disabled catalog entries are not selectable.

- `swarm/scripts/portal_logic_acceptance.mjs`
  - Verifies Gemini and Meta Muse are disabled in the Add Model provider dropdown and labeled `(coming soon)`.

- `swarm/scripts/portal_modal_surface_audit.mjs`
  - Verifies Add Model modal text includes coming-soon provider labels.

## Verification

```bash
python3 swarm/scripts/portal_static_gate.py
node --check /tmp/brighto-portal.js
node --check swarm/scripts/portal_logic_acceptance.mjs
node --check swarm/scripts/portal_modal_surface_audit.mjs
git diff --check
bash swarm/scripts/portal_logic_acceptance.sh
bash swarm/scripts/portal_modal_surface_audit.sh
```

Evidence:

```text
swarm/out/portal_logic_acceptance-provider-coming-soon-231235.log
swarm/out/portal_modal_surface_audit-coming-soon-provider-231250.log
```
