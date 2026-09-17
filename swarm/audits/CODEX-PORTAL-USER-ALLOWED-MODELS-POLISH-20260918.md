# CODEX Portal user allowed models polish — 2026-09-18

## Finding

The User Portal dashboard showed allowed models as a flat hint line below the key/team pills. It worked, but it looked unfinished on mobile and would become hard to scan when a key is scoped to multiple or long model names.

## Change

- `static/index.html`
  - Replaced the flat `Allowed models: ...` sentence with a dedicated allowed-models card inside the user hero.
  - Shows a clear count (`1 allowed`, `All enabled routes`, or `+N more`).
  - Renders model names as wrapped/copy-safe pills with full `title` values for long names.

- `swarm/scripts/portal_user_journey_audit.mjs`
  - Captures allowed-models card geometry.
  - Fails if the user dashboard regresses to flat text.
  - Fails if allowed model pills clip or overflow.

## Verification

Runtime target:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443
```

Checks run:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_user_journey_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
python3 swarm/scripts/portal_static_gate.py
node --check swarm/scripts/portal_user_journey_audit.mjs
```

Results:

- User journey audit: `PASS`, `failures=0`, `consoleErrors=0`
- Full-page audit: `PASS`, `failures=0`, `consoleErrors=0`
- Polish audit: `PASS`, `failures=0`, `consoleErrors=0`
- Modal surface audit rerun: `PASS`, `failures=0`, `consoleErrors=0`
- Static gate: `PORTAL_STATIC_GATE PASS`
- JavaScript syntax check: pass

Artifacts:

```text
swarm/out/playwright/20260918-061431-portal-user-journey-audit
swarm/out/playwright/20260918-061531-portal-modal-surface-audit
```

Measured allowed-model card:

```text
Desktop width: 670px, model pill unclipped
Mobile width: 312px, model pill unclipped
```

## Status

Ready to embed into the production Docker image after commit/build/push.
