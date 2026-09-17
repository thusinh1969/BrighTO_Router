# CODEX audit — API key table long-label polish

Date: 2026-09-18
Role: Codex auditor/implementer
Scope: Portal API Keys table on desktop and mobile

## Root cause

The API Keys table used card-style grid rows on desktop, but owner/team/scope labels were still allowed to wrap or clip through broad table CSS. A realistic long owner or scoped model name could look cut off instead of intentionally compacted.

## Fix applied

- Applied the existing `compact-line` display contract to API key owner, team, and scope labels.
- Strengthened `compact-line` selector coverage for key-list/model-list cells that have broader table overrides.
- Updated `portal_polish_audit.mjs` to seed a long owner and long scoped model name, then assert one-line ellipsis behavior with full value preserved through title/aria-label.

## Verification

Commands run against the live HTTPS container at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
```

Results:

- `portal-polish-audit`: PASS
- `portal-full-page-audit`: PASS

Visual evidence reviewed:

- `swarm/out/playwright/20260918-031510-portal-full-page-audit/desktop-1440-keys.png`

