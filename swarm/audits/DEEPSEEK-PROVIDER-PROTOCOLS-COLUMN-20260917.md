# DeepSeek — provider protocols shown in Providers table — 2026-09-17

Verdict: Providers table now shows the supported protocols per provider (derived from format +
name heuristic), closing the last 'provider template presets' UI gap. Self-audit green.

## Done
- Providers table column 'Protocols' lists supported presets per provider: anthropic ->
  'Anthropic Messages'; OpenAI-compatible -> 'OpenAI Chat, Completions, Embeddings, Local Chat,
  Custom'.
- (Previously this round set) route wizard filters protocol dropdown by provider + searchable/
  manual model entry fallback for Anthropic.

## Anthropic model-list status
- preview-models already sends x-api-key + anthropic-version and parses data[].id, matching
  Anthropic /v1/models shape. Not live-tested (no Anthropic key on hand); manual model entry
  covers the gap.

## Self-audit (all green)
- cargo fmt + clippy -D warnings, release build.
- node --check portal JS: PASS.
