# Preview-2 adapters

This branch adds adapter endpoints without changing the existing chat/completions hot path.

Implemented and tested in this branch:

| Task | Public BrighTO endpoint | Route protocol | Request shape | Status |
|---|---|---|---|---|
| Embeddings | `/v1/embeddings` | `openai_embeddings` | OpenAI-compatible JSON | Existing route, now covered by adapter integration tests |
| Rerank | `/v1/rerank` | `openai_rerank`, `cohere_rerank`, `voyage_rerank`, `jina_rerank` | JSON with `model`, `query`, `documents`, optional `top_n` | Mock/integration tested; real provider smoke still required |
| ASR / speech-to-text | `/v1/audio/transcriptions` | `openai_audio_transcriptions` | OpenAI-compatible multipart form upload | Mock/integration tested; real provider smoke still required |

The router still does not run models. It forwards to a configured provider or local service, applies client-key auth, route policy, budget/concurrency limits, and usage logging. It does not store vectors, rerank documents, audio files, transcripts, prompts, or provider response bodies.

## Live provider smoke tests

Use `scripts/adapter_smoke.py` to prove a provider API key, endpoint, and model with tiny direct requests. It reads keys from environment or `.env`, never prints provider keys, and skips missing providers.

```bash
python3 scripts/adapter_smoke.py --provider jina --task embedding
python3 scripts/adapter_smoke.py --provider jina --task rerank
python3 scripts/adapter_smoke.py --provider all --task all
python3 scripts/adapter_smoke.py --provider openai --task asr --file tests/fixtures/asr_smoke.wav
```

Provider key env vars:

| Provider | Env key | Tasks |
|---|---|---|
| OpenAI | `OPENAI_API_KEY` | embedding, ASR |
| Jina AI | `JINA_API_KEY` | embedding, rerank |
| Voyage AI | `VOYAGE_API_KEY` | embedding, rerank |
| Cohere | `COHERE_API_KEY` | rerank |

Keep stress tests on mock providers. Live provider smoke should stay small and cheap.

## Client smoke tests through BrighTO-Router

These commands call BrighTO-Router as a client app. They require a BrighTO client API key and a public model route that already exists.

Embeddings:

```bash
python3 test_router.py \
  --mode embeddings \
  --router http://127.0.0.1:18080 \
  --api-key lc-... \
  --model <public-embedding-route> \
  --text "BrighTO embedding smoke test"
```

Rerank:

```bash
python3 test_router.py \
  --mode rerank \
  --router http://127.0.0.1:18080 \
  --api-key lc-... \
  --model <public-rerank-route> \
  --text "router speed" \
  --document "BrighTO-Router is a fast Rust gateway" \
  --document "Bananas are yellow fruit" \
  --top-n 1
```

ASR / transcription:

```bash
python3 test_router.py \
  --mode asr \
  --router http://127.0.0.1:18080 \
  --api-key lc-... \
  --model <public-asr-route> \
  --file tests/fixtures/asr_smoke.wav
```

Raw curl equivalents:

```bash
curl -sS http://127.0.0.1:18080/v1/embeddings \
  -H "Authorization: Bearer lc-..." \
  -H "Content-Type: application/json" \
  -d '{"model":"<public-embedding-route>","input":"hello"}'
```

```bash
curl -sS http://127.0.0.1:18080/v1/rerank \
  -H "Authorization: Bearer lc-..." \
  -H "Content-Type: application/json" \
  -d '{"model":"<public-rerank-route>","query":"router speed","documents":["fast rust router","slow proxy"],"top_n":1}'
```

```bash
curl -sS http://127.0.0.1:18080/v1/audio/transcriptions \
  -H "Authorization: Bearer lc-..." \
  -F "model=<public-asr-route>" \
  -F "file=@tests/fixtures/asr_smoke.wav"
```

## Route creation status

The Rust API and integration tests support adapter protocols. The Portal still needs a dedicated task-type wizard before adapter route creation should be considered user-friendly.

For now:

- Chat, completions, embeddings, and Anthropic routes remain the main Portal flow.
- Rerank and ASR routes should be created through admin/API tooling or migration scripts until the Portal wizard has task-specific Test Connection.
- Do not save an enabled adapter route unless the exact endpoint has been tested through BrighTO-Router.

## Provider notes

- OpenAI-compatible embeddings use `/v1/embeddings`.
- Cohere rerank uses provider path `/v2/rerank`. Configure its backend Base URL with the `/v2` prefix so BrighTO's incoming `/v1/rerank` maps correctly.
- Voyage and Jina rerank use `/v1/rerank`.
- ASR currently expects OpenAI-compatible `/v1/audio/transcriptions` multipart behavior.
- Native Qwen/Alibaba rerank or ASR should wait for exact provider API proof before coding a dedicated adapter.

## Validation already in this branch

- `cargo fmt --check`
- `cargo check --locked`
- `cargo clippy --locked --all-targets -- -D warnings`
- `python3 -m py_compile test_router.py scripts/adapter_smoke.py`
- `./scripts/test_postgres.sh`

`./scripts/test_postgres.sh` includes integration tests for embeddings, rerank, ASR multipart, streaming chat, large non-stream uploads, PostgreSQL ledger writing, and fallback behavior.
