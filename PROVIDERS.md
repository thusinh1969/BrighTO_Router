# Provider setup

BrighTO-Router preview-2, intended to become `main` after final feedback, keeps provider setup simple: choose a task type first, then choose a provider preset, test the exact endpoint, and save one model route. The same Portal flow covers chat, embeddings, rerank, and ASR/transcription.

A **provider catalog entry** is only a preset: display name, default Base URL, protocol family, and optional `.env` key name. It is not an active route.

A **model route** is what clients use. It maps one public model name to one upstream provider model, with its task type, provider API key/reference, price, limits, and enabled/disabled state.

## Provider catalog

The catalog is configured by `PROVIDER_CATALOG` in `.env`. The default catalog includes:

| Provider | Default Base URL | Protocol family | Env key |
|---|---|---|---|
| OpenAI | `https://api.openai.com` | OpenAI-compatible | `OPENAI_API_KEY` |
| Anthropic | `https://api.anthropic.com` | Anthropic Messages | `ANTHROPIC_API_KEY` |
| Gemini | `https://generativelanguage.googleapis.com/v1beta/openai` | OpenAI-compatible | `GEMINI_API_KEY` |
| DeepSeek | `https://api.deepseek.com` | OpenAI-compatible | `DEEPSEEK_API_KEY` |
| Kimi | `https://api.moonshot.ai/v1` | OpenAI-compatible | `KIMI_API_KEY` |
| Qwen | `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` | OpenAI-compatible chat/embedding; DashScope rerank adapter uses workspace root URL | `QWEN_API_KEY` or `DASHSCOPE_API_KEY` |
| Z.AI | `https://api.z.ai/api/paas/v4` | OpenAI-compatible | `ZAI_API_KEY` |
| OpenRouter | `https://openrouter.ai/api/v1` | OpenAI-compatible | `OPENROUTER_API_KEY` |
| Jina AI | `https://api.jina.ai` | Embedding/rerank adapter | `JINA_API_KEY` |
| Voyage AI | `https://api.voyageai.com` | Embedding/rerank adapter | `VOYAGE_API_KEY` |
| Cohere | `https://api.cohere.com/v2` | Rerank adapter | `COHERE_API_KEY` |
| Meta Muse | `https://api.meta.ai/v1` | OpenAI-compatible | `META_MUSE_API_KEY` |
| Custom LLM | `http://127.0.0.1:8088/v1` | OpenAI-compatible | `CUSTOM_LLM_API_KEY` |

**OpenAI-compatible** means the backend accepts OpenAI-style routes such as `/v1/chat/completions`, `/v1/embeddings`, `/v1/audio/transcriptions`, or `/v1/models` depending on the selected task. Rerank providers are selected by task type because several providers use different request shapes.


## Default seeded provider endpoints

`./start.sh seed` inserts missing provider endpoint templates into the database so the Portal **Providers** screen is useful on first run. These endpoints are templates and are inserted disabled; creating an enabled model route still happens in **Models & Routes → Add model** after **Test connection** passes.

Seeded adapter endpoint templates include:

| Endpoint template | Base URL | Typical task type in Add model | Notes |
|---|---|---|---|
| `qwen` | `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` | Embedding | Use `qwen3.7-text-embedding` for the current text embedding smoke. |
| `qwen-rerank` | `https://dashscope-intl.aliyuncs.com` | Rerank | Use `qwen3-rerank`; do not use `/compatible-mode/v1` for rerank. |
| `jina` | `https://api.jina.ai` | Embedding or Rerank | Use Jina embedding/rerank model names from the wizard suggestions. |
| `voyage` | `https://api.voyageai.com` | Embedding or Rerank | Free trial accounts may need slower testing because of rate limits. |
| `cohere` | `https://api.cohere.com/v2` | Rerank | Cohere rerank maps BrighTO `/v1/rerank` to provider `/v2/rerank`. |

## Add a model route

In the Portal:

1. Open **Models & Routes**.
2. Click **Add model**.
3. Choose **Task type**: Chat / LLM, Embedding, Rerank, or ASR / transcription.
4. Pick a provider preset or **Custom LLM**.
5. Enter the Base URL.
6. Paste the provider API key, or leave it blank to use the provider `.env` key when it is configured.
7. Click **Load models** when available, or use the task-specific suggestion/manual model name.
8. Click **Test connection**.
9. Save enabled only after the test passes.

The Portal automatically creates or reuses the provider endpoint for the Base URL. You do not need to create a provider first.

## Local OpenAI-compatible endpoint

For llama.cpp, vLLM, LiteLLM, or another local OpenAI-compatible server, choose **Custom LLM** and use a Base URL such as:

```text
http://127.0.0.1:8088/v1
```

If the local endpoint does not require auth, leave API key blank. BrighTO saves that route as no-auth local routing.

## URL handling

Both host-only and SDK-style Base URLs work:

| Base URL | Incoming route | Forwarded URL |
|---|---|---|
| `https://api.openai.com` | `/v1/chat/completions` | `https://api.openai.com/v1/chat/completions` |
| `https://api.moonshot.ai/v1` | `/v1/chat/completions` | `https://api.moonshot.ai/v1/chat/completions` |
| `https://example.com/compatible-mode/v1` | `/v1/models` | `https://example.com/compatible-mode/v1/models` |

## Client API keys

Provider API keys are different from client API keys.

- Provider API key: used by BrighTO to call OpenAI, Anthropic, DeepSeek, or another upstream.
- Client API key: used by your app/team to call BrighTO.

Create client keys in **API Keys**. Admin can view and copy them again later.

## Preview-2 adapter providers

Embeddings, rerank, and ASR/transcription are first-class preview-2 setup flows. The Portal task-type wizard uses task-specific model suggestions and Test Connection probes instead of assuming every provider supports `/v1/models`. Provider catalog entries are templates only; provider keys are supplied per route from `.env` or pasted in the Add Model wizard.

Qwen rerank needs special handling: embeddings can use the OpenAI-compatible `/compatible-mode/v1` Base URL, while rerank uses DashScope workspace endpoints. In the Portal, choose **Qwen + Rerank**, then enter `https://dashscope-intl.aliyuncs.com` or your workspace root Base URL such as `https://<workspace>.<region>.maas.aliyuncs.com`. For live smoke tests, set `QWEN_RERANK_BASE_URL` to that same root URL.

Useful provider key placeholders are present in `.env.example`:

- `JINA_API_KEY`
- `VOYAGE_API_KEY`
- `COHERE_API_KEY`
- `DASHSCOPE_API_KEY`

See [ADAPTERS.md](ADAPTERS.md) for current adapter endpoints and smoke commands.
