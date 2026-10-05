# Agent clients

Use your existing agent CLI with a BrighTO client API key and a public model route or Model Group name. The router forwards the selected native API protocol, including streaming, tool calls, and tool results. A BrighTO SDK is optional. Agent software is not bundled into the router image.

## Choose the matching task

| Client configuration | Portal task | Request endpoint | Client base URL |
|---|---|---|---|
| Claude Code | Anthropic Messages | `/v1/messages` | `http://llm-host.local:18080` |
| Codex | OpenAI Responses | `/v1/responses` | `http://llm-host.local:18080/v1` |
| Hermes, custom provider in Chat Completions mode | Chat | `/v1/chat/completions` | `http://llm-host.local:18080/v1` |
| OpenClaw, `openai-completions` provider mode | Chat | `/v1/chat/completions` | `http://llm-host.local:18080/v1` |

OpenClaw's `openai-completions` mode here means Chat Completions, not the legacy `/v1/completions` API.

In **Models & Routes → Add model route**, select the task, provider, URL, provider model, and provider key if required. Custom endpoints can use a blank provider key. Test connection and save enabled. For a Model Group, select existing tested routes of the same compatible protocol.

The **API model name** is what the client sends. It may differ from the provider's model name. Use a client API key from **API Keys**, never the admin key or a provider key. Base URLs may include `/v1`; the router avoids duplicating that suffix when contacting upstreams.

The upstream must implement the selected protocol and support the tools your agent uses. Chat support does not imply Responses or Anthropic support. BrighTO does not automatically convert these protocols or repair malformed upstream streams. A connection test proves one basic inference request; the tool smoke below checks a real multi-turn agent workflow.

## Claude Code

Create an Anthropic Messages route, then set:

```bash
export ANTHROPIC_BASE_URL=http://llm-host.local:18080
export ANTHROPIC_API_KEY='<brighto-client-api-key>'
export ANTHROPIC_DEFAULT_SONNET_MODEL='<public-messages-model>'
export ANTHROPIC_DEFAULT_HAIKU_MODEL='<public-messages-model>'
export ANTHROPIC_DEFAULT_OPUS_MODEL='<public-messages-model>'
claude --model '<public-messages-model>'
```

Claude's optional `/v1/messages/count_tokens` endpoint is not implemented by BrighTO; the tested client falls back to estimation. Native Messages streaming and tool results require a compatible backend. See [Claude gateway documentation](https://code.claude.com/docs/en/llm-gateway-connect).

## Codex

Create an OpenAI Responses route. Add a custom provider to `~/.codex/config.toml`, keeping unrelated settings intact:

```toml
model = "<public-responses-model>"
model_provider = "brighto"

[model_providers.brighto]
name = "BrighTO-Router"
base_url = "http://llm-host.local:18080/v1"
env_key = "BRIGHTO_API_KEY"
wire_api = "responses"
requires_openai_auth = false
supports_websockets = false
```

```bash
export BRIGHTO_API_KEY='<brighto-client-api-key>'
codex
```

This uses HTTP Responses streaming. BrighTO does not expose a Responses WebSocket transport. Use a backend with a complete native Responses contract, including function calls and tool-result input. A local model with only reliable Chat Completions support is not sufficient. See [Codex gateway configuration](https://learn.chatgpt.com/docs/enterprise/connect-to-a-gateway).

## Hermes

Set the custom model section in your Hermes configuration:

```yaml
model:
  provider: custom
  default: '<public-chat-model>'
  base_url: http://llm-host.local:18080/v1
  api_key: '<brighto-client-api-key>'
  api_mode: chat_completions
```

Run `hermes chat`. Keep the configuration private because it contains a client key. The chosen Chat route must support tool calling. See [Hermes model configuration](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuring-models.md).

## OpenClaw

Merge this provider and model selection into your existing OpenClaw configuration. Set context and output limits to the upstream model's actual supported limits; the values below are examples.

```json
{
  "models": {
    "mode": "merge",
    "providers": {
      "brighto": {
        "baseUrl": "http://llm-host.local:18080/v1",
        "apiKey": "${BRIGHTO_API_KEY}",
        "api": "openai-completions",
        "request": {"allowPrivateNetwork": true},
        "models": [{
          "id": "<public-chat-model>",
          "name": "Team Chat",
          "reasoning": false,
          "input": ["text"],
          "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
          "contextWindow": 32768,
          "maxTokens": 1024
        }]
      }
    }
  },
  "agents": {"defaults": {"model": {"primary": "brighto/<public-chat-model>"}}}
}
```

```bash
export BRIGHTO_API_KEY='<brighto-client-api-key>'
openclaw agent --local --agent main --message 'Reply OK in one sentence.'
```

`allowPrivateNetwork: true` opts in to a **trusted private/LAN router**; without it, the tested OpenClaw client blocks private addresses before sending a request. Omit it for public addresses. Configure `reasoning`, `input`, and model limits to match your upstream. The zero cost fields are client-side example estimates, not a claim that inference is free. See [OpenClaw custom providers](https://docs.openclaw.ai/gateway/config-tools/custom-providers).

## HTTPS

For a publicly trusted certificate, use the HTTPS base URL with normal certificate verification. For a local certificate generated by `start.sh`, copy the public `ssl/ca.pem` to the client and create the combined trust bundle described in [HTTPS setup](https://github.com/thusinh1969/BrighTO_Router/blob/main/HTTPS.md). This preserves normal public HTTPS trust alongside your local CA. Set the applicable variables:

```bash
export NODE_EXTRA_CA_CERTS=/absolute/path/to/client-ca-bundle.pem
export CODEX_CA_CERTIFICATE=/absolute/path/to/client-ca-bundle.pem
export SSL_CERT_FILE=/absolute/path/to/client-ca-bundle.pem
export REQUESTS_CA_BUNDLE=/absolute/path/to/client-ca-bundle.pem
```

Use the hostname covered by the certificate. Do not disable TLS verification. [HTTPS setup](https://github.com/thusinh1969/BrighTO_Router/blob/main/HTTPS.md) explains certificate creation and renewal.

## Repeat a real tool test

[Agent smoke instructions](https://github.com/thusinh1969/BrighTO_Router/blob/main/smoke/agents/README.md) exercise installed CLIs with a random file: the model requests a tool, the CLI reads the file, and the model returns its exact contents. Temporary configuration keeps the test separate from your normal settings. Missing CLIs, failed tools, and incomplete answers fail the test.

Verified client versions on 2026-10-05: Claude Code `2.1.289`, Codex `0.157.1`, Hermes revision `93c9360a8da592cee43e8895403e91c7a920b0ce`, and OpenClaw `2026.9.8`. Each completed the file-reading workflow over HTTP and HTTPS with certificate verification. Chat and Messages used a local llama.cpp model; Codex used native OpenAI Responses. This verifies those workflows, not every client version, model, optional agent feature, or upstream extension.

Only inference POST endpoints are exposed for Responses. Response retrieval, deletion, compaction, and cross-provider session affinity are not implemented. Stateful requests referencing upstream response IDs require the same upstream; use a single route when that matters. Model Groups balance compatible protocols, not provider-held session state.
