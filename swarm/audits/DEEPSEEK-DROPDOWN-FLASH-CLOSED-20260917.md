# DeepSeek — remaining provider dropdown + row-flash closed; both gates green — 2026-09-17

Verdict: Closed the last two FINAL-audit UI items (provider dropdown hygiene + row feedback).
Both acceptance gates PASS against live https after rebuild+recreate.

## This round
- Route wizard provider dropdown: hides disabled providers by default with a 'Show disabled'
  toggle; shows name + (id) to disambiguate duplicate provider names.
- Provider table row flashes briefly after Save (so admin sees the change landed).

## Status of CODEX FINAL action list (all addressed)
1. live docker stale -> fixed (rebuilt+recreated every change).
2. provider copy -> fixed.
3. provider type dropdown -> fixed.
4. route protocol/auth validation -> fixed.
5. load-models quick-create await+rerender -> fixed.
6. team unlimited budget -> null -> fixed.
7. API key edit (PATCH + UI) -> fixed.
8. models table shows provider/model/protocol/auth/context/maxout/prices -> fixed.
9. provider dropdown hide disabled + name/id -> fixed (this round).
10. settings density compact first -> fixed.
11. compact K/M/B formatter everywhere -> fixed.

## Verification (live https)
- python3 swarm/scripts/portal_static_gate.py -> PASS.
- bash swarm/scripts/portal_polish_audit.sh -> PASS (bugs []).
- cargo fmt + clippy -D warnings, release build, 60 lib + 4 integration tests: PASS.
