# DeepSeek — Provider UX + full action list closed; both gates green — 2026-09-17

Verdict: Closed the CODEX urgent provider-edit UX items and the FINAL portal audit action list.
Both acceptance gates PASS against live https.

## Fixed
1. Provider row actions always visible: actions column is sticky-right; Weight + Max columns added
   so Edit changes are visible; stale render already fixed.
2. Wrong key copy removed: Providers page now says providers are endpoint templates and the API
   key is entered per model route.
3. Provider Type dropdown (OpenAI/Anthropic/Gemini/DeepSeek/Kimi/Qwen/Z.AI/OpenRouter/Muse/Local/
   Custom-OpenAI/Custom-Anthropic) fills default URL + maps to format; protocols filtered by
   type (OpenAI no longer advertises Local Chat).
4. Route modal validates protocol/auth: anthropic_messages + none blocked (except local/custom);
   local_openai_chat + bearer blocked.
5. Provider Load models quick-create now awaits + rerender (routes stays an array).
6. Team Unlimited now sends budget:null (clears persisted token budget).
7. API key edit added: PATCH /admin/keys/{id} (owner/team/allowed models/expiry/rpm/concurrency/
   budget/enabled) without regenerating secret; Edit button in API Keys table.
8. Models & Routes table shows provider, provider model, protocol, auth, context, max output,
   price in/out, timeout.
9. Settings density lists Compact (default) first.

## Verification (live https, both green)
- python3 swarm/scripts/portal_static_gate.py -> PASS (10/10).
- bash swarm/scripts/portal_polish_audit.sh -> PASS: formatters 1K/50K/1M, cardBig 25px,
  panelPadding 14px, density compact, localStorage persisted, provider create/edit/delete = 1
  panel each, usage 251.3K/200.1K/50.1K, no comma counts, no console errors.
- cargo fmt + clippy -D warnings, release build, 60 lib + 4 integration tests: PASS.
