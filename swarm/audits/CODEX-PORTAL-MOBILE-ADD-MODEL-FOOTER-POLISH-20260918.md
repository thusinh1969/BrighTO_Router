# Codex audit — mobile Add model footer polish

Date: 2026-09-18
Scope: Portal Add model modal, mobile viewport.

## Problem found

The mobile Add model modal kept Test/Save actions visible with a sticky footer, but the footer overlapped the public-to-provider model mapping preview. The overlap made the form look unfinished and made the mapping harder to read on a 390px-wide mobile viewport.

## Fix applied

- Kept the sticky action footer because Test connection and Save state must stay visible on mobile.
- Compacted the mobile wizard step copy so the three steps stay on one row.
- Kept the Provider model input and Load models button on one mobile row.
- Reduced only the mobile model mapping preview height and spacing.
- Made the mobile route footer background effectively opaque so lower optional content does not visually bleed through the footer.
- Extended the modal Playwright audit to fail when the sticky footer covers the visible model mapping preview.

## Verification

Commands run against the live HTTPS Portal at `https://127.0.0.1:18443`:

```bash
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_modal_surface_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_full_page_audit.sh
BRIGHTO_BASE_URL=https://127.0.0.1:18443 ./swarm/scripts/portal_polish_audit.sh
python3 swarm/scripts/portal_static_gate.py
```

Results:

- `portal_modal_surface_audit`: PASS, 0 failures, 0 console errors.
- `portal_full_page_audit`: PASS, 0 failures, 0 console errors.
- `portal_polish_audit`: PASS, 0 failures, 0 console errors.
- `portal_static_gate`: PASS.

Key evidence from modal audit after the fix:

```text
mobile-add-model footerCoveredImportant=[]
```
