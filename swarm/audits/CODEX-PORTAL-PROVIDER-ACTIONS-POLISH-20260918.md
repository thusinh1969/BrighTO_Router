# CODEX Portal provider action polish — 2026-09-18

## Finding

The Providers page had the correct actions, but on wide desktop screens the action buttons rendered as a 2x2 block. That made the provider card look cramped and less professional than the Models route cards.

## Change

- `static/index.html`
  - Keeps provider actions as 2 columns on medium desktop/tablet where space is limited.
  - Uses a single 4-button row on wide desktop (`>= 1200px`).
  - Gives the action column enough width so Route, Disable, Edit, and Delete stay aligned.

- `swarm/scripts/portal_full_page_audit.mjs`
  - Records provider action button geometry.
  - Fails wide desktop Providers if the four actions wrap to multiple rows.

## Verification

Runtime target:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443
```

Checks run:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
python3 swarm/scripts/portal_static_gate.py
node --check swarm/scripts/portal_full_page_audit.mjs
```

Results:

- Full-page audit: `PASS`, `failures=0`, `consoleErrors=0`
- Polish audit: `PASS`, `failures=0`, `consoleErrors=0`
- Static gate: `PORTAL_STATIC_GATE PASS`
- JavaScript syntax check: pass

Artifact:

```text
swarm/out/playwright/20260918-054849-portal-full-page-audit
```

Measured desktop provider actions:

```text
Route   y=366 width=65
Disable y=366 width=65
Edit    y=366 width=65
Delete  y=366 width=65
```

## Status

Ready to embed into the production Docker image after commit/build/push.
