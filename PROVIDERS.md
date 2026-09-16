# Provider setup

BrighTO-Router seeds provider templates so a new team does not have to write SQL before trying the product.

The templates are disabled by default. Add a provider key, enable the provider in the portal, fetch or type model names, then create model routes.

## Seeded templates

| Provider name | Base URL seeded in DB | Key variable in `.env` | Format |
|---|---|---|---|
| `openai` | `https://api.openai.com` | `OPENAI_API_KEY` | OpenAI-style |
| `anthropic` | `https://api.anthropic.com` | `ANTHROPIC_API_KEY` | Anthropic Messages |
| `gemini` | `https://generativelanguage.googleapis.com/v1beta/openai` | `GEMINI_API_KEY` | OpenAI-style |
| `deepseek` | `https://api.deepseek.com` | `DEEPSEEK_API_KEY` | OpenAI-style |
| `kimi` | `https://api.moonshot.ai/v1` | `KIMI_API_KEY` | OpenAI-style |
| `qwen` | `https://dashscope-intl.aliyuncs.com/compatible-mode/v1` | `QWEN_API_KEY` | OpenAI-style |
| `zai` | `https://api.z.ai/api/coding/paas/v4` | `ZAI_API_KEY` | OpenAI-style |
| `openrouter` | `https://openrouter.ai/api/v1` | `OPENROUTER_API_KEY` | OpenAI-style |
| `meta-muse` | `https://api.meta.ai/v1` | `META_MUSE_API_KEY` | OpenAI-style |
| `custom-openai` | `http://127.0.0.1:8000/v1` | `CUSTOM_LLM_API_KEY` | OpenAI-style |

**OpenAI-style** means the backend accepts routes such as `/v1/chat/completions` and usually returns models from `/v1/models`.

## Fast path for a new provider

```bash
./start.sh set-key openai sk-your-key
./start.sh restart
```

Then in the portal:

1. Enter `ADMIN_MASTER_KEY`.
2. Click **Load providers**.
3. Enable the provider.
4. Click **Fetch models**.
5. Click a model to create a route.
6. Create a team API key.

The router never returns provider API keys from the admin API. It only reports whether the configured env/file reference resolves to a non-empty value.

## Custom OpenAI-compatible endpoint

Use `custom-openai` for vLLM, llama-server, LiteLLM, local gateways, or any compatible service.

Set the key value if the endpoint needs one:

```bash
./start.sh set-key custom-openai your-key
```

The router expects a non-empty backend key before forwarding to a backend. If your local endpoint ignores authentication, set `CUSTOM_LLM_API_KEY` to a dummy value such as `local-dev-key`.

Update the base URL in the portal. Both host-only URLs and SDK-style URLs are supported:

| Base URL | Incoming route | Forwarded URL |
|---|---|---|
| `https://api.openai.com` | `/v1/chat/completions` | `https://api.openai.com/v1/chat/completions` |
| `https://api.moonshot.ai/v1` | `/v1/chat/completions` | `https://api.moonshot.ai/v1/chat/completions` |
| `https://example.com/compatible-mode/v1` | `/v1/models` | `https://example.com/compatible-mode/v1/models` |

## Model fetch behavior

The portal fetches models by calling the provider model-list endpoint through the admin API. This works for OpenAI-style providers that expose `/models` with a `data[].id` response.

If a provider does not expose that shape, type the model name manually and create the route. Routing itself does not require model fetch.
