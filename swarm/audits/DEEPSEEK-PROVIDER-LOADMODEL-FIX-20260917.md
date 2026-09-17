# DeepSeek — provider model-load + dropdown fixed (user-reported) — 2026-09-17

Verdict: Fixed the user-reported confusion: removed the broken 'Load models' from Providers (it
had no key -> immediate 401); model loading now lives only in the Route wizard where the route
key is entered. Consolidated duplicate providers and re-enabled the two real ones.

## Fixed
- Removed the Providers-table 'Load models' button + quick-create (was calling
  /admin/backends/{id}/models with no provider key -> 401). Providers are secret-free templates.
- DB cleanup: consolidated 5x 'Local Qwen' -> one 'Local llama.cpp' (127.0.0.1:8088, enabled);
  2x 'DeepSeek V4 Pro' -> one (enabled); removed local-llama-qwen / -audit duplicates; removed
  all pw-*/crud-*/verify-*/real-test test artifacts.
- Route wizard provider dropdown now lists exactly the enabled providers with (id):
  'Local llama.cpp (id 14)', 'DeepSeek V4 Pro (id 19)'; disabled templates hidden behind
  'Show disabled'.
- Route wizard 'Load models' remains (uses route-level key via preview-models; keyless for local).

## Verification (live https)
- Route wizard dropdown = ['Local llama.cpp (id 14)','DeepSeek V4 Pro (id 19)']; Load models button
  present in wizard only.
- portal_static_gate -> PASS; portal_polish_audit -> PASS (bugs []).
- healthz ok, container healthy.
