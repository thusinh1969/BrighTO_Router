# DeepSeek — route wizard: protocol filtered + searchable/manual model entry — 2026-09-17

Verdict: Route wizard now filters the protocol dropdown by provider (anthropic -> only Anthropic
Messages; OpenAI-compatible -> the OpenAI family + local/custom), and the provider-model field is
a searchable input with manual entry fallback. Self-audit green; image rebuild in progress.

## Done
- protocolsForBackend(): anthropic-format (or anthropic-named) provider -> anthropic_messages
  only; otherwise the OpenAI family (chat/completions/embeddings) + local/custom (no dead
  anthropic option on OpenAI providers).
- Provider select change re-populates the protocol dropdown and re-derives the auth default.
- Provider model field is now <input list=datalist> instead of a closed <select>: Load models
  fills the datalist suggestions, and the admin can also type a model name manually when the
  provider does not expose /models (Anthropic fallback).

## Self-audit (all green)
- cargo fmt + clippy -D warnings, release build.
- node --check portal JS: PASS.

## Remaining
- Anthropic Messages live model-list (currently manual entry fallback; preview-models already
  sends x-api-key + anthropic-version).
- Provider template 'verified vs experimental' per-provider preset metadata (seeds).
