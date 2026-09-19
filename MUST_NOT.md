# MUST NOT: Adapter Safety Rules

BrighTO-Router can expand beyond chat, but expansion must not damage the core promise: an ultra-fast, simple, self-hosted router with a small production stack.

These rules are mandatory for embeddings, reranking, ASR, TTS, image, video, billing, and any future capability.

## Core rule

All non-chat capability must be implemented as an adapter.

```text
Client
  -> BrighTO-Router core
     -> authenticate
     -> enforce budget, rate limit, and concurrency policy
     -> find route
     -> dispatch to adapter by route type or path
     -> forward to provider/backend
     -> extract minimal usage metadata
     -> write usage ledger asynchronously
```

The core router owns auth, policy, route lookup, credential lookup, timing, and usage ledger. The adapter owns request shape, provider-specific headers, provider-specific response shape, provider-specific usage extraction, and connection tests.

## Must not break the fast LLM hot path

- Must not add extra database calls to the current chat/completion hot path.
- Must not require Redis for normal production operation.
- Must not make chat routing depend on embedding/rerank/audio code.
- Must not parse large request bodies more deeply than the current route needs.
- Must not normalize or transform provider responses unless that adapter explicitly owns the route shape.
- Must not add global middleware that slows every request for a feature used by one adapter.
- Must not make Portal/API startup require any commercial provider account.

## Must not turn the router into a model server

BrighTO-Router routes to providers and local model servers. It does not run embedding, reranker, ASR, TTS, image, or video models inside the core router process.

Do not add:

- model weights
- GPU inference runtime
- vector index
- vector database
- media transcoding pipeline
- background job queue for model inference
- provider SDK dependency when plain HTTP is enough

Local models are supported by routing to an external local server such as llama.cpp, vLLM, TEI, Faster-Whisper, or an OpenAI-compatible service.

## Must not store customer content

The router may store usage metadata. It must not store customer content by default.

Do not persist:

- chat prompts
- message arrays
- tool payloads
- uploaded files
- image/audio/video bytes
- embedding vectors
- reranker documents
- ASR audio
- TTS output audio
- provider response bodies
- model answers
- full provider error bodies

If transcript/content retention is ever required, it must be a separate explicit enterprise feature with encryption, redaction, retention policy, and audited access. It must not be silently added to the open-source hot path.

## Must not over-complicate production dependencies

The open-source production stack is:

- Rust router
- PostgreSQL
- Docker Compose by default
- optional Kubernetes manifests

Do not add a required dependency unless a concrete production use case proves it and the user accepts the tradeoff.

Specifically:

- No required Redis.
- No required object storage.
- No required vector DB.
- No required message queue.
- No required provider-specific SDK.

## Must not hide provider behavior

Adapters should make provider differences explicit and testable.

Do not pretend all providers have the same shape when they do not. Instead, define a small adapter contract and map each provider honestly.

Examples:

- OpenAI-compatible embedding uses `/v1/embeddings`.
- Jina/Voyage embedding can use `/v1/embeddings` but may add provider-specific fields.
- Cohere rerank uses `/v2/rerank`.
- Voyage rerank uses `/v1/rerank`.
- Gemini embedding uses `models/gemini-embedding-2:embedContent`, not OpenAI shape.
- ASR may use multipart upload; do not force it into JSON chat.

## Must not save broken model routes

The Portal and Admin API must not allow a new adapter route to be marked enabled unless its test succeeds, except when the operator explicitly saves it disabled.

A good route creation flow is:

1. Choose task type: Chat, Embedding, Rerank, ASR, TTS.
2. Choose provider preset or Custom.
3. Enter base URL and provider API key reference/value.
4. Load models when the provider supports listing.
5. Select exactly one model or enter one custom model id.
6. Run test connection with a minimal safe sample.
7. Save enabled only after test passes.

## Must not damage public-source clarity

Docs must say what works now, what is planned, and what is not supported.

Do not overclaim:

- Do not claim ASR/TTS until adapter endpoints pass real tests.
- Do not claim multimodal embedding for every provider.
- Do not claim vector search.
- Do not claim reranking until `/v1/rerank` or provider-specific rerank adapter passes real tests.

## Required validation before merge

Every adapter feature needs:

- unit tests for route type and adapter mapping
- mock upstream tests for success, provider error, timeout, and bad payload
- Portal create/test/save smoke path if UI changes
- real-provider smoke script gated by environment variables
- documentation with exact curl examples
- ledger verification that usage metadata is recorded without content

