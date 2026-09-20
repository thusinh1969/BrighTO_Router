# Preview-2 adapters

This branch adds adapter endpoints without changing the existing chat/completions hot path.

Implemented and tested in this branch:

| Task | Public BrighTO endpoint | Route protocol | Request shape | Status |
|---|---|---|---|---|
| Embeddings | `/v1/embeddings` | `openai_embeddings` | OpenAI-compatible JSON | Mock/integration tested; live OpenAI, Qwen, Jina, and Voyage smoke passed |
| Rerank | `/v1/rerank` | `openai_rerank`, `qwen_rerank`, `cohere_rerank`, `voyage_rerank`, `jina_rerank` | JSON with `model`, `query`, `documents`, optional `top_n` | Mock/integration tested; live Qwen, Jina, Voyage, and Cohere smoke passed |
| ASR / speech-to-text | `/v1/audio/transcriptions` | `openai_audio_transcriptions` | OpenAI-compatible multipart form upload | Mock/integration tested; live OpenAI smoke passed with repo WAV fixtures |

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

The Portal model wizard supports task-specific route creation for Chat, Embedding, Rerank, and ASR. Test Connection uses the selected task's exact endpoint shape before enabling the route:

- Chat: one tiny chat/messages request.
- Embedding: one short `/v1/embeddings` request and vector-dimension validation.
- Rerank: one `/v1/rerank` request with three tiny documents and score validation.
- ASR: one `/v1/audio/transcriptions` multipart request using `tests/fixtures/asr_smoke.wav`.

Save enabled only after Test Connection passes. Save draft remains available for disabled routes.

## Provider notes

- OpenAI-compatible embeddings use `/v1/embeddings`.
- Cohere rerank uses provider path `/v2/rerank`; the default Cohere Base URL includes `/v2`, so BrighTO's incoming `/v1/rerank` maps to `/v2/rerank`.
- Voyage and Jina rerank use `/v1/rerank`. BrighTO maps public `top_n` to Voyage `top_k` when needed.
- Qwen/DashScope rerank is not OpenAI-compatible. BrighTO accepts public `/v1/rerank`, then maps `qwen3-rerank` to `/compatible-api/v1/reranks`; `qwen3.7-text-rerank`, `qwen3-vl-rerank`, and `gte-rerank-v2` map to `/api/v1/services/rerank/text-rerank/text-rerank`. Configure the Base URL as `https://dashscope-intl.aliyuncs.com` for the international shared endpoint, or as your workspace root such as `https://<workspace>.<region>.maas.aliyuncs.com`.
- Qwen `tongyi-embedding-vision-flash` is a real multimodal embedding model, but it uses DashScope multimodal embedding APIs, not the OpenAI-compatible `/v1/embeddings` text route. It should be a dedicated future adapter rather than a misleading model suggestion in the current text embedding flow.
- ASR currently expects OpenAI-compatible `/v1/audio/transcriptions` multipart behavior.
- Additional Qwen/Alibaba ASR or media adapters should wait for exact provider API proof before coding.

## Validation already in this branch

- `cargo fmt --check`
- `cargo check --locked`
- `cargo clippy --locked --all-targets -- -D warnings`
- `python3 -m py_compile test_router.py scripts/adapter_smoke.py`
- `./scripts/test_postgres.sh`

`./scripts/test_postgres.sh` includes integration tests for embeddings, rerank, ASR multipart, streaming chat, large non-stream uploads, PostgreSQL ledger writing, and fallback behavior.
