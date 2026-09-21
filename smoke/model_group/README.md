# Model Group load-balancing smoke

Preview-3 Model Groups let one public OpenAI-compatible chat model name fan out to multiple compatible chat endpoints. Client applications keep calling `/v1/chat/completions` with the same `model` value; BrighTO-Router chooses the backend.

## Mock smoke

Safe deterministic smoke with no paid provider keys. It starts three local OpenAI-compatible mock chat endpoints, creates one public model group `coding-fast`, and verifies weighted round-robin routing plus endpoint-specific provider model rewrite.

Run:

```bash
./smoke/model_group/run_mock.sh
```

Expected sequence for weights `DeepSeek=3`, `local llama.cpp=1`, `OpenAI=2`:

```text
deepseek-v4-pro
deepseek-v4-pro
deepseek-v4-pro
qwen3.8-flash-next
gpt-5.6-mini
gpt-5.6-mini
```

This smoke measures router/load-balancer path latency against mock endpoints. It does not measure paid model inference speed.

## Live OpenAI chat smoke: DeepSeek V4 Pro + local llama.cpp

This is the real-world example for preview-3. It starts temporary PostgreSQL and a temporary BrighTO-Router, then creates one public model group `coding-fast-live` with two OpenAI-compatible chat endpoints:

| Backend | Provider model | Auth | Default base URL |
| --- | --- | --- | --- |
| DeepSeek | `deepseek-v4-pro` | `DEEPSEEK_API_KEY` | `https://api.deepseek.com` |
| local llama.cpp | `qwen3.8-flash-next` | none | `http://127.0.0.1:8088/v1` |

Run after local llama.cpp is already running:

```bash
DEEPSEEK_API_KEY=sk-... ./smoke/model_group/run_live_openai_chat.sh
```

Or put `DEEPSEEK_API_KEY` in `.env` and run:

```bash
./smoke/model_group/run_live_openai_chat.sh
```

Useful overrides:

```bash
./smoke/model_group/run_live_openai_chat.sh \
  --local-base http://127.0.0.1:8088/v1 \
  --local-model qwen3.8-flash-next \
  --deepseek-model deepseek-v4-pro \
  --public-model coding-fast-live
```

The live smoke calls the public model twice and checks `x-router-backend`: first DeepSeek V4 Pro, then local llama.cpp, using PostgreSQL-backed round-robin state with `ROUTE_COUNTER_BLOCK_SIZE=1` so the sequence is easy to verify.
