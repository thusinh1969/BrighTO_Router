# CODEX audit — Portal SOTA polish round 2 gate PASS — 2026-09-17

## Verdict

PASS for this polish round. The Portal is materially more usable than the prior build: long model/provider names no longer collide with status/actions, request logs are readable, mobile tables no longer create horizontal page scroll, and the Add model modal keeps its action bar visible while scrolling.

This is still not a final claim that the whole product is SOTA. It is a verified visual/UX improvement pass on the current single-file Portal.

## What changed

1. `static/index.html` table layout was changed from rigid fixed columns to controlled cells:
   - Public model names can wrap up to two lines with copy still available.
   - Provider and provider-model cells get primary/subtext hierarchy.
   - Route settings are grouped into a compact metadata grid.
   - Provider connection rows now expose connection type and keep actions visible.
2. Request logs no longer render as an overcrowded 11-column table. They now render as request cards:
   - model + timestamp header
   - status pill
   - Team / Key / In-Out / Tok/s / Cost / Router / Duration KPI blocks
   - Tok/s is visually emphasized because it is operationally meaningful.
3. Mobile card/table rules were hardened:
   - no whole-page horizontal overflow
   - long model/provider names break safely
   - colgroups are disabled on mobile
   - rows/cells clamp to viewport width
4. Add model modal now has a sticky action footer so Cancel / Test connection / Save remain reachable in long forms.
5. Chart legend labels are constrained to avoid long model names damaging layout.

## Verification run

All current Portal gates passed after the changes:

- `swarm/scripts/portal_empty_state_audit.sh`
- `swarm/scripts/portal_logic_acceptance.sh`
- `swarm/scripts/portal_visual_audit.sh`
- `swarm/scripts/portal_polish_audit.sh`
- `swarm/scripts/portal_full_page_audit.sh`

Screenshots from the passing run are under the latest `swarm/out/playwright/*portal-full-page-audit` directory.

## Remaining design bar

The next high-value pass should improve the product-level dashboard hierarchy: cost/run-rate, provider health, and route readiness should be more obvious at a glance. Current screens are now readable and stable enough for that pass.
