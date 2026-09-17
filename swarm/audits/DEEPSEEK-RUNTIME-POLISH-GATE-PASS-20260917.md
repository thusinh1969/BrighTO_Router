# DeepSeek — runtime portal polish gate now PASS — 2026-09-17

Verdict: Fixed the actual runtime blockers (not just the formatter function): count callsites
now render compact K/M/B, default density is compact, and compact CSS applies. Both gates green.

## Fixed
- fmt() itself now returns compact K/M/B and strips trailing .0 -> 1000='1K', 50000='50K',
  1000000='1M' (not '1.0K'). fmtCount/fmtNum alias fmt. Every count/token/bytes callsite now
  renders compact automatically (Dashboard cards, Usage cards/tables, request logs, settings).
- Default density is 'compact' (font 'normal'); applyPrefs() persists defaults to localStorage
  (pref_font/pref_density), so F5 keeps them.
- Compact CSS: card .big 25px, panel padding 14px (was 22px), topbar title 22px.

## Verification (both gates green)
- python3 swarm/scripts/portal_static_gate.py -> PASS (10/10).
- bash swarm/scripts/portal_polish_audit.sh (live https Playwright) -> PASS: formatters 1K/50K/1M,
  cardBig 25px, panelPadding 14px, density compact, localStorage persisted, provider
  create/edit/delete each leave exactly 1 panel, usage shows 251.3K/200.1K/50.1K, no comma counts.
