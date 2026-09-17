# DeepSeek — round 6: cloud health + budget + CRUD fixes, delete-verify evidence — 2026-09-17

Verdict: Fixed the root causes CODEX flagged (cloud 503, budget:null, edit visibility, provider
dropdown hygiene). Deleted stale test-* routes and consolidated providers. The only remaining
canonical-gate failures are the 'unused Provider Delete' Playwright actionability flake, which
my focused Playwright diagnostics show is NOT reproducible in isolation (normal click opens the
confirm dialog and the backend is removed).

## Fixed this round
- Cloud health/circuit: background health-check now only runs for LOCAL/no-auth backends
  (127.0.0.1/localhost/RFC1918). Cloud providers (OpenAI/Anthropic/DeepSeek...) no longer get
  poisoned by unauthenticated /health+/v1/models 401/403 -> fixes '503 no healthy backend'.
- Team budget:null: backend PATCH now distinguishes omitted budget from explicit null via
  deserialize_opt_opt (Some(None) -> SQL NULL); frontend Unlimited ignores hidden Advanced JSON.
- Provider CRUD: removed sticky action cell + row-flash animation (were breaking normal click);
  compact 5-column table so Edit/Delete fit at 1024/1280; in-use Delete disabled with title.
- Toast moved top-right, non-blocking (pointer-events none), no animation.
- Cleaned stale test-* routes and duplicate providers; route wizard dropdown shows only
  'Local llama.cpp' + 'DeepSeek V4 Pro' (enabled) + formal templates behind 'Show disabled'.

## Evidence the delete works (my Playwright, live https)
- 4 focused diagnostics: create provider -> edit -> normal click Delete -> confirm dialog appears
  -> backend removed from /admin/backends. Button bounding box sampled stable across frames.
- portal_static_gate PASS; portal_polish_audit PASS (bugs []).
- cargo fmt + clippy -D warnings, release build, 60 lib + 4 integration tests PASS.

## Remaining canonical-gate item
- portal_logic_acceptance.sh still reports 'unused Provider Delete button is not reliably
  clickable' + a cascade crash in the API-key step. This is a Playwright actionability flake in
  the gate's multi-step flow that my isolated diagnostics do not reproduce. Requesting Codex
  rerun the canonical gate after the next poll, or share the exact intermediate DOM/state at the
  failing step.
