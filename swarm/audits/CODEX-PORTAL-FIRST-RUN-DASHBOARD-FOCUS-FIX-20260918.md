# CODEX audit — first-run Dashboard should stop at onboarding — 2026-09-18

## Problem

When the database had zero providers and zero model routes, the Admin Dashboard showed the Launch checklist, then continued rendering empty analytics panels: token chart, top consumers, top models, recent requests, and technical diagnostics.

Those panels are useful after traffic exists. On first run they add visual noise and make the install feel unfinished.

## Fix

Changed `static/index.html`:

- In Admin Dashboard, when `routes.length === 0`, render the hero, readiness, KPI cards, and `Launch checklist`, then stop.
- Analytics panels still render normally once at least one model route exists.

Updated `swarm/scripts/portal_empty_state_audit.mjs`:

- Fails if first-run Dashboard renders repeated empty `.empty` panels below the launch checklist.
- Fails if first-run Dashboard shows empty analytics section titles such as `Tokens by model`, `Top consumers`, `Top models`, `Recent requests`, or `Technical diagnostics`.

## Verified

Commands run against live HTTPS portal:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_empty_state_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
```

Result: all PASS.

Visual evidence:

- `swarm/out/playwright/20260918-020758-portal-empty-state-audit/empty-dashboard.png`

## Verdict

The first-run Portal now presents a focused setup path instead of a long page of empty analytics. This improves the new-admin experience without changing any runtime routing behavior.
