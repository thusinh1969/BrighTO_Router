# CODEX audit — mobile chart legend polish

Date: 2026-09-18
Role: Codex auditor/implementer
Scope: Portal dashboard/usage chart legend on mobile

## Root cause

Chart legends on mobile rendered as narrow content-sized pills. Long model names were technically compacted, but visually looked cramped and arbitrary inside chart cards.

## Fix applied

- Kept desktop legend behavior unchanged.
- On mobile, render chart legend items as full-width rows inside the card.
- Let the text area flex and ellipsize cleanly while keeping the color dot fixed.
- Kept full model names available through existing title/aria-label behavior.

## Verification

Commands run against the live HTTPS container at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
```

Results:

- `portal-full-page-audit`: PASS
- `portal-polish-audit`: PASS

Visual evidence reviewed:

- `swarm/out/playwright/20260918-030827-portal-full-page-audit/mobile-390-dashboard.png`

