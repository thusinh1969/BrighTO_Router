# Adapter Plan: Embeddings, Reranking, and ASR

This plan expands BrighTO-Router without breaking the current fast LLM router. All new capabilities are adapters. TTS is intentionally deferred because it adds binary output, voice options, longer-running responses, and provider-specific media behavior.

## Objective

Add world-standard serving for:

1. Embeddings
2. Reranking
3. ASR / speech-to-text transcription

The router remains a router. It does not store vectors, run models, build indexes, transcode media, or keep customer content.

## Design principle

Keep the hot path small:

```text
auth -> policy -> route lookup -> adapter -> provider -> usage ledger
```

Adapter code must be isolated so chat/completion performance remains unchanged.

## New route types

Add explicit task/route types. Current protocol names can remain for compatibility, but the UI and docs should expose task type clearly.

| Task type | Public endpoint | Initial adapter scope |
|---|---|---|
| Chat | `/v1/chat/completions`, `/v1/completions`, `/v1/messages` | Existing implementation |
| Embedding | `/v1/embeddings` | OpenAI-compatible first; provider-specific pass-through where safe |
| Rerank | `/v1/rerank` | BrighTO normalized request, provider adapters underneath |
| ASR | `/v1/audio/transcriptions` | OpenAI-compatible multipart first; provider-specific adapters later |
| TTS | Future | Defer |

## Adapter contract

Each adapter should implement the same conceptual contract:

```text
AdapterSpec
  task_type
  provider_family
  public_path
  provider_path
  auth_header_policy
  request_validation
  request_transform
  response_transform_or_passthrough
  usage_extraction
  test_payload
```

Keep implementation simple in Rust. Do not introduce a heavy plugin runtime. A trait or enum dispatch is enough.

## Database changes

Add only minimal durable fields needed for routing and analytics.

Recommended migration:

- Add route/task type if existing `protocol` is too narrow.
- Add usage metadata columns only if required:
  - `request_units BIGINT DEFAULT 1`
  - `input_bytes BIGINT DEFAULT 0`
  - `output_bytes BIGINT DEFAULT 0`
  - `vector_count BIGINT DEFAULT 0`
  - `vector_dim BIGINT DEFAULT 0`
  - `document_count BIGINT DEFAULT 0`
  - `returned_count BIGINT DEFAULT 0`
  - `audio_seconds DOUBLE PRECISION DEFAULT 0`

Do not store actual vectors, documents, audio, or transcripts.

If adding many nullable columns feels noisy, use one `usage_extra JSONB` field for adapter-specific counters. Prefer concrete columns only for fields the Portal/Grafana will query often.

## Provider test accounts needed

To test commercial endpoints properly, register API keys for these providers:

| Provider | Need now? | Why |
|---|---:|---|
| OpenAI | Yes | Baseline `/v1/embeddings` and `/v1/audio/transcriptions` behavior. |
| Jina AI | Yes | Strong embedding + rerank provider, good future multimodal embedding story. |
| Voyage AI | Yes | Strong retrieval/code embeddings and rerankers. |
| Cohere | Yes | Rerank baseline with mature `/v2/rerank`. |
| Alibaba Cloud Model Studio / Qwen | Yes | Important Qwen embedding/rerank coverage and Asia/multilingual use. |
| Google Gemini | Later | Multimodal embedding API shape differs; useful after text embedding/rerank is stable. |

Environment variables to support:

```text
OPENAI_API_KEY=
JINA_API_KEY=
VOYAGE_API_KEY=
COHERE_API_KEY=
DASHSCOPE_API_KEY=
GEMINI_API_KEY=
```

For Qwen/Alibaba, also support workspace endpoints because production docs recommend workspace-dedicated domains:

```text
DASHSCOPE_BASE_URL=https://dashscope-intl.aliyuncs.com/compatible-mode/v1
QWEN_WORKSPACE_BASE_URL=https://{WorkspaceId}.{region}.maas.aliyuncs.com/compatible-mode/v1
```

## Provider adapter matrix

### Embedding

| Provider | Adapter family | Provider endpoint | Initial test model |
|---|---|---|---|
| OpenAI | `openai_embeddings` | `/v1/embeddings` | `text-embedding-3-small` |
| Qwen / Alibaba | `openai_embeddings` first | `/compatible-mode/v1/embeddings` | `qwen3.7-text-embedding` or `qwen3.7-text-embedding-flash` |
| Jina | `jina_embeddings` or OpenAI-like HTTP | `/v1/embeddings` | current text embedding model from account/model list |
| Voyage | `voyage_embeddings` | `/v1/embeddings` | `voyage-4-lite` or `voyage-code-4` |
| Gemini | `gemini_embedding` later | `models/gemini-embedding-2:embedContent` | `gemini-embedding-2` |

### Rerank

Use a BrighTO public normalized endpoint:

```http
POST /v1/rerank
Authorization: Bearer <brighto-client-key>
Content-Type: application/json

{
  "model": "public-rerank-route-name",
  "query": "What is BrighTO-Router?",
  "documents": ["doc 1", "doc 2"],
  "top_n": 2
}
```

Return normalized response:

```json
{
  "object": "rerank.list",
  "model": "public-rerank-route-name",
  "results": [
    {"index": 0, "relevance_score": 0.99},
    {"index": 1, "relevance_score": 0.12}
  ],
  "usage": {
    "input_tokens": 123,
    "document_count": 2,
    "returned_count": 2
  }
}
```

Provider mappings:

| Provider | Adapter family | Provider endpoint | Notes |
|---|---|---|---|
| Cohere | `cohere_rerank` | `/v2/rerank` | Query + documents + top_n; docs recommend not sending more than 1,000 docs/request. |
| Voyage | `voyage_rerank` | `/v1/rerank` | Query + documents + model. |
| Jina | `jina_rerank` | `/v1/rerank` | Query + documents + model. |
| Qwen / Alibaba | `qwen_rerank` | native endpoint to verify | Needs exact API shape from current account/docs before coding final adapter. |
| Custom | `custom_rerank` | operator-defined | Must support mapping mode or BrighTO normalized shape. |

### ASR

Public endpoint should follow OpenAI-compatible shape first:

```http
POST /v1/audio/transcriptions
Authorization: Bearer <brighto-client-key>
Content-Type: multipart/form-data

file=@sample.wav
model=<public-asr-route-name>
language=en
response_format=json
```

Initial provider mappings:

| Provider | Adapter family | Provider endpoint | Notes |
|---|---|---|---|
| OpenAI | `openai_asr` | `/v1/audio/transcriptions` | Baseline. |
| Custom OpenAI-compatible ASR | `openai_asr` | `/v1/audio/transcriptions` | Works for local Whisper/Faster-Whisper servers that mimic OpenAI. |
| Qwen/Alibaba Paraformer | later `dashscope_asr` | native API | Requires separate native adapter. |
| Others | later | provider-specific | Add only after real endpoint proof. |

ASR usage logging:

- `input_bytes`
- `audio_seconds` if known or provider returns it
- `total_ms`
- `status`
- `error_class`
- no transcript persistence by default

## Portal requirements

Add route creation as a single clear workflow.

Fields:

- Task type: Chat, Embedding, Rerank, ASR.
- Provider preset.
- Base URL.
- API key source/value.
- Model id.
- Price fields by unit:
  - chat: price per 1M input/output tokens
  - embedding: price per 1M tokens or per 1K requests if provider bills that way
  - rerank: price per 1K search units or per 1M tokens, provider-specific label
  - ASR: price per minute or per audio hour
- Test connection button.
- Save enabled only after test passes.
- Save disabled allowed for incomplete setup.

Dashboard must show task type filters:

- Chat
- Embedding
- Rerank
- ASR
- All

Usage table should show meaningful metrics by task:

| Task | Important metrics |
|---|---|
| Chat | tokens, tokens/sec, duration, provider/model/team |
| Embedding | input tokens, vector count, vector dimension, duration |
| Rerank | document count, returned count, input tokens/search units, duration |
| ASR | audio seconds, input bytes, duration |

## Test strategy

### Unit tests

- Parse task/adapter protocol values.
- Map public path to provider path.
- Extract usage for each provider sample response.
- Reject invalid rerank payloads.
- Reject ASR payloads over body limit.
- Ensure no content fields are written to ledger.

### Mock provider tests

Add mock endpoints for:

- `/v1/embeddings`
- `/v1/rerank`
- `/v2/rerank`
- `/v1/audio/transcriptions`

Test:

- success
- provider 400
- provider 401
- provider 429
- timeout
- malformed provider response
- client abort where practical

### Real provider smoke tests

Create one script:

```bash
python3 scripts/adapter_smoke.py --provider openai --task embedding
python3 scripts/adapter_smoke.py --provider jina --task embedding
python3 scripts/adapter_smoke.py --provider jina --task rerank
python3 scripts/adapter_smoke.py --provider voyage --task embedding
python3 scripts/adapter_smoke.py --provider voyage --task rerank
python3 scripts/adapter_smoke.py --provider cohere --task rerank
python3 scripts/adapter_smoke.py --provider openai --task asr --file tests/fixtures/audio/hello.wav
```

The script must skip providers whose API key env var is missing. It must never print keys.

### Portal smoke

Use Playwright or existing Portal smoke style to test:

1. Login.
2. Add embedding route.
3. Test connection.
4. Save.
5. Call `/v1/embeddings` through router.
6. See usage row.
7. Add rerank route.
8. Test connection.
9. Save.
10. Call `/v1/rerank` through router.
11. See usage row.
12. Add ASR route disabled if no real ASR key is available.

## Documentation required

Update docs only after tests pass.

Files:

- `README.md`
- `PROVIDERS.md`
- `INSTALL.md`
- new `ADAPTERS.md` if content becomes large
- `test_router.py` examples or new `test_adapter.py`

Docs must include:

- exact curl for embedding
- exact curl for rerank
- exact curl for ASR
- provider setup table
- what is logged and what is not stored
- pricing unit explanation
- current limitations

## Rollout order

### Step 1: Embedding hardening

The router already has `/v1/embeddings`. Make it first-class:

- Portal task type displays Embedding clearly.
- Test connection for embedding route.
- Usage metrics show vector count/dim if possible.
- Real-provider smoke for OpenAI and Qwen/Jina/Voyage as keys become available.

### Step 2: Rerank adapter

Add `/v1/rerank` with normalized BrighTO request/response.

Initial providers:

1. Cohere
2. Voyage
3. Jina
4. Qwen after exact endpoint proof
5. Custom normalized rerank

### Step 3: ASR adapter

Add `/v1/audio/transcriptions` for OpenAI-compatible multipart.

Initial providers:

1. OpenAI
2. Custom OpenAI-compatible Whisper/Faster-Whisper

Add Qwen/Alibaba Paraformer only after exact native API proof.

### Step 4: TTS later

Do not include TTS in this coding wave. TTS needs separate binary response handling and voice/provider options.

## Done criteria

This work is complete only when:

- Chat path still passes all existing tests.
- New adapter tests pass.
- At least one real embedding provider passes.
- At least one real rerank provider passes.
- ASR OpenAI-compatible mock passes; real provider passes if key is available.
- Portal create/test/save flow works for task types.
- Usage ledger records useful metadata without storing content.
- Docs are clear enough for a new user to test with one command.

