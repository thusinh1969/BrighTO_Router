# Preview-3 adapters: embeddings, rerank, ASR, and Model Groups

This is the latest preview-3 adapter and routing scope: embeddings stay on the OpenAI-compatible route, rerank plus ASR/transcription are task-specific adapters, and Model Groups add same-type route load balancing without changing the client API call.

Implemented and tested in preview-3:

| Task | Public BrighTO endpoint | Route protocol | Request shape | Status |
|---|---|---|---|---|
| Embeddings | `/v1/embeddings` | `openai_embeddings` | OpenAI-compatible JSON | Mock/integration tested; live OpenAI, Qwen, Jina, and Voyage smoke passed |
| Rerank | `/v1/rerank` | `openai_rerank`, `qwen_rerank`, `cohere_rerank`, `voyage_rerank`, `jina_rerank` | JSON with `model`, `query`, `documents`, optional `top_n` | Mock/integration tested; live Qwen, Jina, Voyage, and Cohere smoke passed |
| ASR / speech-to-text | `/v1/audio/transcriptions` | `openai_audio_transcriptions` | OpenAI-compatible multipart form upload | Mock/integration tested; live OpenAI smoke passed with repo WAV fixtures |
| Model Groups | Same endpoint as selected route type | `model_group_<type>` | Normal request shape for chat, embeddings, rerank, or ASR using the public group model name | Mock/integration tested; Portal browser smoke creates source routes, saves a group from existing routes, and lists it |

The router still does not run models. It forwards to a configured provider or local service, applies client-key auth, route policy, budget/concurrency limits, and usage logging. It does not store vectors, rerank documents, audio files, transcripts, prompts, or provider response bodies.

## Live provider smoke tests

Use `scripts/adapter_smoke.py` to prove a provider API key, endpoint, and model with tiny direct requests. It reads keys from environment or `.env`, never prints provider keys, and skips missing providers.

```bash
python3 scripts/adapter_smoke.py --provider qwen --task embedding
python3 scripts/adapter_smoke.py --provider qwen --task rerank
python3 scripts/adapter_smoke.py --provider jina --task embedding
python3 scripts/adapter_smoke.py --provider jina --task rerank
python3 scripts/adapter_smoke.py --provider all --task all
python3 scripts/adapter_smoke.py --provider openai --task asr --file tests/fixtures/asr_smoke.wav
```

Provider key env vars:

| Provider | Env key | Tasks |
|---|---|---|
| OpenAI | `OPENAI_API_KEY` | embedding, ASR |
| Qwen/DashScope | `QWEN_API_KEY` or `DASHSCOPE_API_KEY` | embedding, rerank |
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
  --api-key sk-brighto-... \
  --model <public-embedding-route> \
  --text "BrighTO embedding smoke test"
```

Provider shortcuts when you use the documented public route names:

```bash
python3 test_router.py --list-presets
python3 test_router.py --provider qwen --mode embeddings --text "hello"
python3 test_router.py --provider qwen --mode rerank --query "router speed"
python3 test_router.py --provider jina --mode embeddings --text "hello"
python3 test_router.py --provider jina --mode rerank --query "router speed"
python3 test_router.py --provider voyage --mode embeddings --text "hello"
python3 test_router.py --provider voyage --mode rerank --query "router speed"
python3 test_router.py --provider cohere --mode rerank --query "router speed"
```

Rerank:

```bash
python3 test_router.py \
  --mode rerank \
  --router http://127.0.0.1:18080 \
  --api-key sk-brighto-... \
  --model <public-rerank-route> \
  --query "router speed" \
  --document "BrighTO-Router is a fast Rust gateway" \
  --document "Bananas are yellow fruit" \
  --top-n 1
```

ASR / transcription:

```bash
python3 test_router.py \
  --mode asr \
  --router http://127.0.0.1:18080 \
  --api-key sk-brighto-... \
  --model <public-asr-route> \
  --file tests/fixtures/asr_smoke.wav
```

Raw curl equivalents:

```bash
curl -sS http://127.0.0.1:18080/v1/embeddings \
  -H "Authorization: Bearer sk-brighto-..." \
  -H "Content-Type: application/json" \
  -d '{"model":"<public-embedding-route>","input":"hello"}'
```

```bash
curl -sS http://127.0.0.1:18080/v1/rerank \
  -H "Authorization: Bearer sk-brighto-..." \
  -H "Content-Type: application/json" \
  -d '{"model":"<public-rerank-route>","query":"router speed","documents":["fast rust router","slow proxy"],"top_n":1}'
```

```bash
curl -sS http://127.0.0.1:18080/v1/audio/transcriptions \
  -H "Authorization: Bearer sk-brighto-..." \
  -F "model=<public-asr-route>" \
  -F "file=@tests/fixtures/asr_smoke.wav"
```

## Portal setup: how adapter routes differ from chat

Embedding and rerank routes are not selected by changing only the provider. In **Models & Routes → Add model route**, choose the **Task type** first:

1. **Embedding** creates a route for `/v1/embeddings`. The provider model must be an embedding model such as Qwen `qwen3.7-text-embedding`, Jina embedding models, Voyage embedding models, or OpenAI embedding models.
2. **Rerank** creates a route for `/v1/rerank`. The provider model must be a reranker such as Qwen `qwen3-rerank`, Jina reranker, Voyage reranker, or Cohere reranker.
3. **ASR / transcription** creates a route for `/v1/audio/transcriptions` and tests with the small bundled WAV fixture.
4. **Chat / LLM** remains the normal `/v1/chat/completions` or Anthropic Messages flow.

Provider endpoint templates in **Providers** are only Base URLs. A route becomes usable only after the task-specific **Test connection** passes and the route is saved enabled.

## Portal setup: Model Groups

A Model Group is not a new provider type. It is one unique API model name backed by multiple existing tested routes of the same type. Use it for backend load balancing and failover while keeping client code stable.

In **Models & Routes → Create Model Group**:

1. Choose the model type.
2. Set the public group model name.
3. Choose round robin or weighted round robin.
4. Add existing tested routes from the compatible-route dropdown.
5. Save enabled after at least two compatible routes are selected.

Provider URL, provider model, auth mode, and provider key/reference stay on the source routes. The group wizard does not ask for provider keys. Preview-3 groups do not mix protocol shapes: chat routes group with chat, embeddings with embeddings, rerank with rerank, and ASR with ASR.

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

## Validation already in preview-3

- `cargo fmt --check`
- `cargo check --locked`
- `cargo clippy --locked --all-targets -- -D warnings`
- `python3 -m py_compile test_router.py scripts/adapter_smoke.py scripts/api_matrix_smoke.py scripts/adapter_router_smoke.py scripts/anthropic_smoke.py`
- `python3 scripts/api_matrix_smoke.py` for deterministic mock coverage of OpenAI-compatible chat, Anthropic Messages, embeddings, rerank, ASR multipart, and protocol guard behavior.
- `python3 scripts/adapter_router_smoke.py` for live OpenAI chat/embeddings/ASR plus Qwen, Jina, Voyage, and Cohere adapter coverage when keys are present.
- `python3 scripts/anthropic_smoke.py` for live Anthropic Messages coverage when `ANTHROPIC_API_KEY` is present.
- `./scripts/test_postgres.sh`

`./scripts/test_postgres.sh` includes integration tests for embeddings, rerank, ASR multipart, streaming chat, large non-stream uploads, PostgreSQL ledger writing, and fallback behavior.
