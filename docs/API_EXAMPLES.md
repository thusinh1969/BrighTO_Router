# BrighTO-Router API examples

These examples show how a client application calls BrighTO-Router after an admin has created and tested routes in the Portal.

Every Portal route has two client-facing facts:

| Portal task | Client endpoint | The client sends `model` as |
|---|---|---|
| Chat Completions | `/v1/chat/completions` | the route or Model Group API model name |
| Completions | `/v1/completions` | the route or Model Group API model name |
| Responses API | `/v1/responses` | the route or Model Group API model name |
| Embeddings | `/v1/embeddings` | the route or Model Group API model name |
| Rerank | `/v1/rerank` | the route or Model Group API model name |
| ASR / transcription | `/v1/audio/transcriptions` | the route API model name |
| System One / Decision | `/v1/systemone` or `/v1/decisions` | the route or Model Group API model name |
| Anthropic Messages | `/v1/messages` | the route API model name |

The Portal decides where that model name goes behind the scenes. Client apps still call the endpoint that matches the request and response shape they use.

## Shared setup

```python
import os
import requests

router = os.getenv("BRIGHTO_ROUTER_URL", "http://127.0.0.1:18080").rstrip("/")
api_key = os.environ["BRIGHTO_API_KEY"]

# For local self-signed HTTPS smoke tests only:
verify_tls = os.getenv("BRIGHTO_INSECURE_TLS") != "1"

headers = {"Authorization": f"Bearer {api_key}"}

def post_json(path: str, payload: dict) -> dict:
    resp = requests.post(
        f"{router}{path}",
        headers={**headers, "Content-Type": "application/json"},
        json=payload,
        timeout=120,
        verify=verify_tls,
    )
    resp.raise_for_status()
    return resp.json()
```

## Chat Completions

Portal task: **Chat Completions (`/v1/chat/completions`)**.

```python
data = post_json("/v1/chat/completions", {
    "model": "<public-chat-route-or-group>",
    "messages": [
        {"role": "user", "content": "Reply OK in one short sentence."}
    ],
    "stream": False,
})

print(data["choices"][0]["message"]["content"])
```

## Completions

Portal task: **Completions (`/v1/completions`)**. Use this for legacy text-completion providers or local endpoints that still expose the Completions API.

```python
data = post_json("/v1/completions", {
    "model": "<public-completions-route-or-group>",
    "prompt": "Reply OK in one short sentence.",
    "max_tokens": 32,
    "stream": False,
})

print(data["choices"][0]["text"])
```

## Responses API

Portal task: **Responses API (`/v1/responses`)**. Use this when your upstream backend exposes OpenAI Responses semantics.

```python
data = post_json("/v1/responses", {
    "model": "<public-responses-route-or-group>",
    "input": "Reply OK in one short sentence.",
    "max_output_tokens": 32,
    "stream": False,
})

print(data.get("output_text") or data["output"][0]["content"][0]["text"])
```

## Embeddings

Portal task: **Embedding (`/v1/embeddings`)**. The router forwards the vector response unchanged.

```python
data = post_json("/v1/embeddings", {
    "model": "<public-embedding-route-or-group>",
    "input": "BrighTO-Router is a fast Rust AI gateway.",
})

vector = data["data"][0]["embedding"]
print("dimensions:", len(vector))
print("preview:", vector[:8])
```

## Rerank

Portal task: **Rerank (`/v1/rerank`)**. The router adapts supported provider-specific rerank shapes behind this endpoint.

```python
data = post_json("/v1/rerank", {
    "model": "<public-rerank-route-or-group>",
    "query": "fast Rust LLM gateway",
    "documents": [
        "BrighTO-Router is an ultra-fast self-hosted Rust AI gateway.",
        "Bananas are yellow fruit.",
        "Rerankers score candidate documents for a query.",
    ],
    "top_n": 2,
})

for row in data.get("results", []):
    print(row)
```

## ASR / transcription

Portal task: **ASR / transcription (`/v1/audio/transcriptions`)**. This endpoint is multipart because audio is a file upload.

```python
with open("tests/fixtures/asr_smoke.wav", "rb") as audio:
    resp = requests.post(
        f"{router}/v1/audio/transcriptions",
        headers=headers,
        data={"model": "<public-asr-route>"},
        files={"file": ("asr_smoke.wav", audio, "audio/wav")},
        timeout=120,
        verify=verify_tls,
    )

resp.raise_for_status()
print(resp.json()["text"])
```

## System One / Decision

Portal task: **System One / Decision (`/v1/systemone`)**. `/v1/decisions` is also accepted as an alias.

```python
data = post_json("/v1/systemone", {
    "model": "<public-systemone-route-or-group>",
    "state": {"message": "I was charged twice for one order."},
    "questions": {
        "duplicate_charge": {
            "type": "noul",
            "instructions": "Does the message report a duplicate charge?",
        },
        "team": {
            "type": "choice",
            "instructions": "Which team should handle this?",
            "criteria": {
                "billing": "payments and refunds",
                "support": "technical help",
            },
        },
    },
})

print(data["answers"])
```

## Anthropic Messages

Portal task: **Chat Completions** with an Anthropic-compatible provider creates an Anthropic Messages route. Client apps call `/v1/messages` when they use Anthropic's request shape.

```python
data = post_json("/v1/messages", {
    "model": "<public-anthropic-route>",
    "max_tokens": 64,
    "messages": [
        {"role": "user", "content": "Reply OK in one short sentence."}
    ],
})

content = data.get("content", [])
print(content[0].get("text") if content else data)
```

## Multimodal chat pass-through

For OpenAI-compatible chat models that accept image or audio content inside Chat Completions JSON, BrighTO-Router forwards the JSON body unchanged.

```python
data = post_json("/v1/chat/completions", {
    "model": "<public-vision-chat-route>",
    "messages": [{
        "role": "user",
        "content": [
            {"type": "text", "text": "Describe this image."},
            {"type": "image_url", "image_url": {"url": "https://example.com/image.jpg"}},
        ],
    }],
    "stream": False,
})

print(data["choices"][0]["message"]["content"])
```

## Quick CLI equivalent

`test_router.py` wraps the same endpoint choices for smoke testing:

```bash
python3 test_router.py --mode chat --model <public-chat-route> --text "Reply OK"
python3 test_router.py --mode completions --model <public-completions-route> --text "Reply OK"
python3 test_router.py --mode responses --model <public-responses-route> --text "Reply OK"
python3 test_router.py --mode embeddings --model <public-embedding-route> --text "hello"
python3 test_router.py --mode rerank --model <public-rerank-route> --query "router speed" --document "fast Rust gateway" --document "slow proxy"
python3 test_router.py --mode asr --model <public-asr-route> --file tests/fixtures/asr_smoke.wav
python3 test_router.py --mode systemone --model <public-systemone-route> --text "I was charged twice"
python3 test_router.py --mode messages --model <public-anthropic-route> --text "Reply OK"
```
