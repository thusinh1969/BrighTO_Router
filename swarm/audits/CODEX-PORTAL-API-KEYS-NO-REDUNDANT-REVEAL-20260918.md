# CODEX audit — API key rows remove redundant Reveal action

Date: 2026-09-18
Commit target: pending
Area: Portal API Keys page

## Root cause fixed

The API Keys table auto-loaded and displayed full API keys for admins, but still rendered a `Reveal` button next to each visible key. Since admins are allowed to see keys again, this created redundant UI and made each row wider and noisier.

## Change made

- When the full key loads successfully, the row now shows only the full key and a copy button.
- The `Reveal` button remains only as a fallback if automatic key load fails.
- Updated Playwright full-page audit: visible key rows must have full key + copy and no ghost Reveal column.
- Updated logic acceptance: verifies full key is visible in the row and copy is available, without requiring a Reveal click.

## Verification

Passed locally after the change:

- `portal_full_page_audit PASS swarm/out/portal_full_page_audit-keys-two-column-row-011824.log`
- `portal_logic_acceptance PASS swarm/out/portal_logic_acceptance-keys-no-redundant-reveal-011720.log`
- `portal_modal_surface_audit PASS swarm/out/portal_modal_surface_audit-keys-no-reveal-final-011855.log`
- `portal_user_journey_audit PASS swarm/out/portal_user_journey_audit-keys-no-reveal-final-011855.log`
- `portal_polish_audit PASS swarm/out/portal_polish_audit-keys-no-reveal-final-011855.log`

## Current evidence

Desktop API key rows now render as a 2-column grid: full key + copy button. No extra 0px Reveal column remains.
