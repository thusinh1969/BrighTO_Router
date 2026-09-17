# CODEX audit — Add Model modal width polish

Date: 2026-09-18
Role: Codex auditor/implementer
Scope: Portal Add Model modal layout

## Root cause

The Add Model flow is the most important admin flow, but the desktop modal was constrained to the same width as simpler modals. This made the provider-model field feel cramped beside the Load models button and reduced readability of the public-model mapping section.

## Fix applied

- Widened only the Add Model modal on desktop to 860px.
- Kept the generic wide modal width unchanged for simpler forms.
- Adjusted the model grid to align fields from the top and give the provider-model side more usable width.
- Mobile behavior remains under the existing 820px responsive rules.

## Verification

Commands run against the live HTTPS container at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
```

Results:

- `portal-modal-surface-audit`: PASS
- `portal-polish-audit`: PASS

Visual evidence reviewed:

- `swarm/out/playwright/20260918-032011-portal-modal-surface-audit/desktop-add-model.png`
- `swarm/out/playwright/20260918-032011-portal-modal-surface-audit/mobile-add-model.png`

