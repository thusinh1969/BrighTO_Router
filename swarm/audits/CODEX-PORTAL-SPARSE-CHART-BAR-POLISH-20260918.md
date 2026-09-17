# CODEX Portal sparse chart bar polish — 2026-09-18

## Finding

Dashboard and Usage charts looked too technical when the data set was sparse. With only one to three days of traffic, the token chart rendered as a very narrow vertical line, which made the Portal feel unfinished and harder for an admin to read quickly.

## Change

- `static/index.html`
  - Detects sparse chart data (`<= 3` day buckets).
  - Uses wider desktop data bars for sparse charts.
  - Marks real chart bars with `.chart-data-bar` so visual audits can measure them.
  - Slightly rounds chart bars to match the rest of the Portal card style.

- `swarm/scripts/portal_full_page_audit.mjs`
  - Captures chart data bar geometry from rendered SVG.
  - Fails desktop Dashboard/Usage if sparse charts regress into skinny bars.

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
swarm/out/playwright/20260918-054339-portal-full-page-audit
```

Measured sparse chart bars after the fix:

- Desktop Dashboard: one bar, rendered width `45px` inside the narrower dashboard chart card.
- Desktop Usage: one bar, rendered width `56px` inside the full usage chart card.
- Mobile remains compact at about `29px`, which fits the smaller viewport.

## Status

Ready to embed into the production Docker image after commit/build/push.
