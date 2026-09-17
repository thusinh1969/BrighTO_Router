# CODEX audit — long label compact-line polish

Date: 2026-09-18
Role: Codex auditor/implementer
Scope: Portal tables for Models & Routes and Provider connections

## Root cause

Long public/provider/model names were previously allowed to wrap with `overflow-wrap:anywhere` inside the card-style admin tables. This made provider names break in the middle of tokens and made the Portal look unpolished when real provider/model names were long.

## Fix applied

- Added a `compact-line` display class with one-line ellipsis behavior.
- Applied it to provider connection names, base URLs, route provider names, and route provider model names.
- Kept full values available through `title` / `aria-label` via the existing `nameNode` helper.
- Extended `portal_polish_audit.mjs` to seed a route/provider with long realistic labels and assert that long table labels render as one-line ellipsis instead of broken wraps.

## Verification

Commands run against the live HTTPS container at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_logic_acceptance.sh
```

Results:

- `portal-polish-audit`: PASS
- `portal-full-page-audit`: PASS
- `portal-modal-surface-audit`: PASS
- `portal-logic-acceptance`: PASS

Visual evidence reviewed:

- `swarm/out/playwright/20260918-030249-portal-full-page-audit/desktop-1440-models.png`
- `swarm/out/playwright/20260918-030249-portal-full-page-audit/desktop-1440-providers.png`

