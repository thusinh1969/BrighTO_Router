# Provider setup

BrighTO-Router keeps provider setup simple: the Portal shows a provider catalog from `.env`, and the database stores only the real connections and model routes you create.

A **provider catalog entry** is only a preset: display name, default Base URL, protocol family, and optional `.env` key name. It is not an active route.

A **model route** is what clients use. It maps one public model name to one upstream provider model, with its provider API key/reference, price, limits, and enabled/disabled state.

## Provider catalog

The catalog is configured by `PROVIDER_CATALOG` in `.env`. The default catalog includes:

| Provider | Default Base URL | Protocol family | Env key |
|---|---|---|---|
| OpenAI | `https://api.openai.com` | OpenAI-compatible | `OPENAI_API_KEY` |
| Anthropic | `https://api.anthropic.com` | Anthropic Messages | `ANTHROPIC_API_KEY` |
| Gemini | `https://generativelanguage.googleapis.com/v1beta/openai` | OpenAI-compatible | `GEMINI_API_KEY` |
| DeepSeek | `https://api.deepseek.com` | OpenAI-compatible | `DEEPSEEK_API_KEY` |
| Kimi | `https://api.moonshot.ai/v1` | OpenAI-compatible | `KIMI_API_KEY` |
| Qwen | `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` | OpenAI-compatible | `QWEN_API_KEY` |
| Z.AI | `https://api.z.ai/api/paas/v4` | OpenAI-compatible | `ZAI_API_KEY` |
| OpenRouter | `https://openrouter.ai/api/v1` | OpenAI-compatible | `OPENROUTER_API_KEY` |
| Meta Muse | `https://api.meta.ai/v1` | OpenAI-compatible | `META_MUSE_API_KEY` |
| Custom LLM | `http://127.0.0.1:8088/v1` | OpenAI-compatible | `CUSTOM_LLM_API_KEY` |

**OpenAI-compatible** means the backend accepts routes such as `/v1/chat/completions` and usually returns models from `/v1/models`.

## Add a model route

In the Portal:

1. Open **Models & Routes**.
2. Click **Add model**.
3. Pick a provider preset or **Custom LLM**.
4. Enter the Base URL.
5. Paste the provider API key, or leave it blank to use the provider `.env` key when it is configured.
6. Click **Load models** and select exactly one model. If model listing is unsupported, type the provider model name manually.
7. Click **Test connection**.
8. Save enabled only after the test passes.

The Portal automatically creates or reuses the provider connection for the Base URL. You do not need to create a provider first.

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
