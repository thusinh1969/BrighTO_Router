#!/usr/bin/env python3
"""Tiny BrighTO-Router smoke client.

Examples:
  python3 test_router.py --model my-model --text "Reply OK"
  python3 test_router.py --router http://SERVER:18080 --api-key sk-brighto-... --model my-model --text "Reply OK"
  python3 test_router.py --mode embeddings --model my-embedding --text "hello"
  python3 test_router.py --mode rerank --model my-reranker --text "search query" --document "doc one" --document "doc two"
  python3 test_router.py --mode asr --model my-asr --file ./sample.wav
  python3 test_router.py --provider qwen --mode embeddings --text "hello"
  python3 test_router.py --provider jina --mode rerank --query "search query"
  python3 test_router.py --model my-vision-model --text "What is this?" --image ./photo.jpg
  python3 test_router.py --model my-audio-model --text "Transcribe briefly" --audio ./sample.wav

This is a client-side helper. It calls BrighTO-Router with a BrighTO client API key.
It never uses provider keys such as OPENAI_API_KEY.
"""

from __future__ import annotations

import argparse
import base64
import json
import mimetypes
import os
import ssl
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

DEFAULT_ROUTER = "http://127.0.0.1:18080"
DEFAULT_DEMO_KEY = "sk-brighto-0123456789abcdef0123456789abcdef"

# Public route-name shortcuts used by the preview-2 smoke flow and docs.
# They are client-side conveniences only. If your Portal route uses a custom public
# model name, pass --model explicitly and ignore these presets.
PROVIDER_ROUTE_PRESETS: dict[str, dict[str, str]] = {
    "openai": {"embeddings": "openai-embedding", "asr": "openai-asr"},
    "qwen": {"embeddings": "qwen-embedding", "rerank": "qwen-rerank"},
    "jina": {"embeddings": "jina-embedding", "rerank": "jina-rerank"},
    "voyage": {"embeddings": "voyage-embedding", "rerank": "voyage-rerank"},
    "cohere": {"rerank": "cohere-rerank"},
}


class CliError(Exception):
    pass


def parse_env_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        return values
    for raw in path.read_text(errors="ignore").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "'\"":
            value = value[1:-1]
        values[key] = value
    return values


def env_value(name: str, env_file_values: dict[str, str]) -> str:
    return os.environ.get(name) or env_file_values.get(name, "")


def first_env(names: list[str], env_file_values: dict[str, str]) -> str:
    for name in names:
        value = env_value(name, env_file_values).strip()
        if value:
            return value
    return ""


def preset_model_name(provider: str, mode: str) -> str:
    provider_key = provider.strip().lower()
    if provider_key not in PROVIDER_ROUTE_PRESETS:
        known = ", ".join(sorted(PROVIDER_ROUTE_PRESETS))
        raise CliError(f"unknown provider preset '{provider}'. Known: {known}")
    task_routes = PROVIDER_ROUTE_PRESETS[provider_key]
    if mode not in task_routes:
        available = ", ".join(sorted(task_routes))
        raise CliError(f"provider preset '{provider}' has no {mode} route shortcut. Available modes: {available}")
    return task_routes[mode]


def print_presets() -> None:
    print("Preview-2 provider route presets")
    print("These are public route names expected after creating routes with the documented names.")
    print("Use --model when your Portal route has a different public name.\n")
    for provider, modes in sorted(PROVIDER_ROUTE_PRESETS.items()):
        for mode, model in sorted(modes.items()):
            print(f"{provider:8s} {mode:10s} -> {model}")


def guess_media_type(path: Path, fallback: str) -> str:
    guessed, _ = mimetypes.guess_type(str(path))
    return guessed or fallback


def data_url(path_text: str, fallback_mime: str) -> str:
    if path_text.startswith(("http://", "https://", "data:")):
        return path_text
    path = Path(path_text)
    if not path.exists() or not path.is_file():
        raise CliError(f"file not found: {path_text}")
    mime = guess_media_type(path, fallback_mime)
    encoded = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:{mime};base64,{encoded}"


def audio_item(path_text: str) -> dict[str, Any]:
    if path_text.startswith(("http://", "https://", "data:")):
        raise CliError("--audio expects a local file path for the OpenAI-style input_audio example")
    path = Path(path_text)
    if not path.exists() or not path.is_file():
        raise CliError(f"file not found: {path_text}")
    ext = path.suffix.lower().lstrip(".") or "wav"
    aliases = {"wave": "wav", "oga": "ogg"}
    fmt = aliases.get(ext, ext)
    allowed = {"wav", "mp3", "m4a", "ogg", "flac", "webm"}
    if fmt not in allowed:
        raise CliError(f"unsupported audio extension '.{ext}'. Use one of: {', '.join(sorted(allowed))}")
    encoded = base64.b64encode(path.read_bytes()).decode("ascii")
    return {"type": "input_audio", "input_audio": {"data": encoded, "format": fmt}}


def redact_large_media(value: Any) -> Any:
    if isinstance(value, dict):
        out: dict[str, Any] = {}
        for k, v in value.items():
            if k == "data" and isinstance(v, str) and len(v) > 80:
                out[k] = f"<base64 {len(v)} chars>"
            elif k == "url" and isinstance(v, str) and v.startswith("data:"):
                head = v.split(",", 1)[0]
                out[k] = f"{head},<base64 redacted>"
            else:
                out[k] = redact_large_media(v)
        return out
    if isinstance(value, list):
        return [redact_large_media(v) for v in value]
    return value


def build_body(args: argparse.Namespace) -> tuple[str, dict[str, Any]]:
    if args.mode == "chat":
        content: str | list[dict[str, Any]]
        if args.image or args.audio:
            items: list[dict[str, Any]] = [{"type": "text", "text": args.text}]
            for image in args.image:
                items.append({"type": "image_url", "image_url": {"url": data_url(image, "image/jpeg")}})
            for audio in args.audio:
                items.append(audio_item(audio))
            content = items
        else:
            content = args.text
        body: dict[str, Any] = {
            "model": args.model,
            "messages": [{"role": "user", "content": content}],
            "stream": False,
        }
        if args.max_tokens is not None:
            body["max_tokens"] = args.max_tokens
        if args.temperature is not None:
            body["temperature"] = args.temperature
        return "/v1/chat/completions", body

    if args.mode == "embeddings":
        if args.image or args.audio:
            raise CliError("embeddings mode accepts text only in this helper")
        return "/v1/embeddings", {"model": args.model, "input": args.text}

    if args.mode == "rerank":
        if args.image or args.audio:
            raise CliError("rerank mode accepts text documents only")
        docs = args.document or [
            "BrighTO-Router is a fast Rust AI gateway.",
            "Bananas are yellow fruit.",
            "Rerankers score documents against a query.",
        ]
        query = (args.query or args.text or "").strip()
        if not query:
            raise CliError("rerank mode requires --query or --text")
        return "/v1/rerank", {
            "model": args.model,
            "query": query,
            "documents": docs,
            "top_n": min(args.top_n or len(docs), len(docs)),
        }

    if args.mode == "messages":
        if args.image or args.audio:
            raise CliError("messages mode in this helper is text-only; use --mode chat for OpenAI-style image/audio JSON")
        return "/v1/messages", {
            "model": args.model,
            "max_tokens": args.max_tokens or 128,
            "messages": [{"role": "user", "content": args.text}],
        }

    raise CliError(f"unknown mode: {args.mode}")


def post_multipart_asr(url: str, api_key: str, model: str, file_path: str, timeout: int, verify_tls: bool) -> tuple[int, dict[str, str], bytes]:
    path = Path(file_path)
    if not path.exists() or not path.is_file():
        raise CliError(f"file not found: {file_path}")
    boundary = "----brighto-router-test-boundary"
    mime = guess_media_type(path, "application/octet-stream")
    file_bytes = path.read_bytes()
    parts = [
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\n{model}\r\n".encode("utf-8"),
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{path.name}\"\r\nContent-Type: {mime}\r\n\r\n".encode("utf-8"),
        file_bytes,
        f"\r\n--{boundary}--\r\n".encode("utf-8"),
    ]
    data = b"".join(parts)
    req = urllib.request.Request(
        url,
        data=data,
        method="POST",
        headers={
            "authorization": f"Bearer {api_key}",
            "content-type": f"multipart/form-data; boundary={boundary}",
            "accept": "application/json",
        },
    )
    ctx = None if verify_tls else ssl._create_unverified_context()
    try:
        with urllib.request.urlopen(req, context=ctx, timeout=timeout) as resp:
            return resp.status, {k.lower(): v for k, v in resp.headers.items()}, resp.read()
    except urllib.error.HTTPError as exc:
        return exc.code, {k.lower(): v for k, v in exc.headers.items()}, exc.read()
    except urllib.error.URLError as exc:
        raise CliError(f"cannot reach router: {exc}") from exc


def post_json(url: str, api_key: str, body: dict[str, Any], timeout: int, verify_tls: bool) -> tuple[int, dict[str, str], bytes]:
    data = json.dumps(body, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        method="POST",
        headers={
            "authorization": f"Bearer {api_key}",
            "content-type": "application/json",
            "accept": "application/json",
        },
    )
    ctx = None if verify_tls else ssl._create_unverified_context()
    try:
        with urllib.request.urlopen(req, context=ctx, timeout=timeout) as resp:
            return resp.status, {k.lower(): v for k, v in resp.headers.items()}, resp.read()
    except urllib.error.HTTPError as exc:
        return exc.code, {k.lower(): v for k, v in exc.headers.items()}, exc.read()
    except urllib.error.URLError as exc:
        raise CliError(f"cannot reach router: {exc}") from exc


def print_chat(data: dict[str, Any]) -> None:
    choices = data.get("choices") or []
    if choices:
        first = choices[0]
        message = first.get("message") if isinstance(first, dict) else None
        if isinstance(message, dict) and message.get("content") is not None:
            content = message.get("content")
            if isinstance(content, str):
                print(content)
            else:
                print(json.dumps(content, ensure_ascii=False, indent=2))
            return
        if isinstance(first, dict) and first.get("text") is not None:
            print(first["text"])
            return
    print(json.dumps(data, ensure_ascii=False, indent=2))


def print_embeddings(data: dict[str, Any]) -> None:
    rows = data.get("data") or []
    first = rows[0] if rows and isinstance(rows[0], dict) else {}
    emb = first.get("embedding") if isinstance(first, dict) else None
    dim = len(emb) if isinstance(emb, list) else 0
    preview = emb[:8] if isinstance(emb, list) else None
    print(f"object: {data.get('object')}")
    print(f"model: {data.get('model')}")
    print(f"vectors: {len(rows)}")
    print(f"first_vector_dimensions: {dim}")
    if preview is not None:
        print("first_vector_preview:", json.dumps(preview))
    if data.get("usage") is not None:
        print("usage:", json.dumps(data["usage"], ensure_ascii=False))


def print_rerank(data: dict[str, Any]) -> None:
    results = data.get("results") or data.get("data") or []
    print(f"results: {len(results) if isinstance(results, list) else 0}")
    if isinstance(results, list):
        for r in results[:10]:
            if isinstance(r, dict):
                print(f"index={r.get('index')} score={r.get('relevance_score', r.get('score'))}")
    if data.get("usage") is not None:
        print("usage:", json.dumps(data["usage"], ensure_ascii=False))
    if data.get("meta") is not None:
        print("meta:", json.dumps(data["meta"], ensure_ascii=False))


def print_asr(data: dict[str, Any]) -> None:
    if data.get("text") is not None:
        print(data["text"])
    else:
        print(json.dumps(data, ensure_ascii=False, indent=2))


def print_messages(data: dict[str, Any]) -> None:
    content = data.get("content")
    if isinstance(content, list):
        texts = []
        for item in content:
            if isinstance(item, dict) and item.get("type") == "text" and item.get("text"):
                texts.append(str(item["text"]))
        if texts:
            print("\n".join(texts))
            return
    print(json.dumps(data, ensure_ascii=False, indent=2))


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Call BrighTO-Router once with chat, embeddings, rerank, ASR, or Anthropic Messages.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""Environment fallback order:
  router URL: BRIGHTO_ROUTER_URL, then BASE_URL from .env, then http://127.0.0.1:18080
  client key: BRIGHTO_ROUTER_API_KEY, ROUTER_API_KEY, BRIGHTO_API_KEY
  model:      BRIGHTO_MODEL

Provider keys such as OPENAI_API_KEY are intentionally ignored. This script tests the router as a client app.

Provider shortcuts use standard public route names created in the docs/smoke flow:
  python3 test_router.py --provider qwen --mode embeddings --text "hello"
  python3 test_router.py --provider qwen --mode rerank --query "router speed"
  python3 test_router.py --provider jina --mode embeddings --text "hello"
  python3 test_router.py --provider jina --mode rerank --query "router speed"
  python3 test_router.py --provider voyage --mode embeddings --text "hello"
  python3 test_router.py --provider voyage --mode rerank --query "router speed"
  python3 test_router.py --provider cohere --mode rerank --query "router speed"
  python3 test_router.py --provider openai --mode asr --file tests/fixtures/asr_smoke.wav
""",
    )
    parser.add_argument("--router", help="Router base URL, for example http://127.0.0.1:18080")
    parser.add_argument("--api-key", help="BrighTO client API key, usually starts with sk-brighto-")
    parser.add_argument("--model", help="Public model route name in BrighTO-Router")
    parser.add_argument("--provider", choices=sorted(PROVIDER_ROUTE_PRESETS), help="Use a preview-2 provider route preset, for example qwen + embeddings -> qwen-embedding")
    parser.add_argument("--list-presets", action="store_true", help="Print preview-2 provider route presets and exit")
    parser.add_argument("--text", default="Reply OK in one short sentence.", help="Text input to send; in rerank mode this is the query unless --query is set")
    parser.add_argument("--query", help="Search query for --mode rerank. Friendly alias; overrides --text for rerank only")
    parser.add_argument("--mode", choices=["chat", "embeddings", "rerank", "asr", "messages"], default="chat", help="Request type")
    parser.add_argument("--image", action="append", default=[], help="Image file path, http URL, https URL, or data URL for OpenAI-style chat JSON")
    parser.add_argument("--audio", action="append", default=[], help="Audio file path for OpenAI-style chat JSON")
    parser.add_argument("--document", action="append", default=[], help="Document text for --mode rerank; repeat for multiple documents")
    parser.add_argument("--top-n", type=int, help="Number of rerank results to return")
    parser.add_argument("--file", help="Audio file path for --mode asr multipart upload")
    parser.add_argument("--max-tokens", type=int, default=128, help="Max output tokens for chat/messages")
    parser.add_argument("--temperature", type=float, help="Optional chat temperature")
    parser.add_argument("--timeout", type=int, default=120, help="HTTP timeout seconds")
    parser.add_argument("--env-file", default=".env", help="Env file to read for local defaults")
    parser.add_argument("--insecure", action="store_true", help="Allow self-signed HTTPS certificates for local smoke tests")
    parser.add_argument("--raw-response", action="store_true", help="Print full JSON response")
    parser.add_argument("--dry-run", action="store_true", help="Print the request that would be sent, with media bytes redacted")
    args = parser.parse_args()
    if args.list_presets:
        print_presets()
        return 0

    env_file_values = parse_env_file(Path(args.env_file))
    router = (args.router or first_env(["BRIGHTO_ROUTER_URL", "BASE_URL"], env_file_values) or DEFAULT_ROUTER).rstrip("/")
    api_key = args.api_key or first_env(["BRIGHTO_ROUTER_API_KEY", "ROUTER_API_KEY", "BRIGHTO_API_KEY"], env_file_values)
    model = args.model or (preset_model_name(args.provider, args.mode) if args.provider else "") or first_env(["BRIGHTO_MODEL"], env_file_values)
    if not api_key:
        raise CliError(
            "missing BrighTO client API key. Pass --api-key sk-brighto-... or set BRIGHTO_ROUTER_API_KEY in .env. "
            f"For a fresh local install, the demo key is {DEFAULT_DEMO_KEY}."
        )
    if api_key.startswith("sk-") and not api_key.startswith("sk-brighto-"):
        raise CliError("this looks like a provider key. Use a BrighTO client key from the Portal API Keys screen, usually sk-brighto-...")
    if not model:
        raise CliError("missing model. Pass --model <public-model-route>, use --provider with a supported --mode, or set BRIGHTO_MODEL in .env")
    args.model = model

    if args.mode == "asr":
        if not args.file:
            raise CliError("--mode asr requires --file ./audio.wav")
        path = "/v1/audio/transcriptions"
        url = router + path
        if args.dry_run:
            print("router:", router)
            print("endpoint:", path)
            print("model:", model)
            print("file:", args.file)
            return 0
        started = time.perf_counter()
        status, headers, raw = post_multipart_asr(url, api_key, model, args.file, args.timeout, verify_tls=not args.insecure)
    else:
        path, body = build_body(args)
        url = router + path
        if args.dry_run:
            print("router:", router)
            print("endpoint:", path)
            print("model:", model)
            print("body:")
            print(json.dumps(redact_large_media(body), ensure_ascii=False, indent=2))
            return 0
        started = time.perf_counter()
        status, headers, raw = post_json(url, api_key, body, args.timeout, verify_tls=not args.insecure)
    elapsed_ms = (time.perf_counter() - started) * 1000

    print(f"status: {status}")
    print(f"elapsed_ms: {elapsed_ms:.1f}")
    for name in ["x-router-request-id", "x-router-backend", "x-router-overhead-ms"]:
        if headers.get(name):
            print(f"{name}: {headers[name]}")

    try:
        data = json.loads(raw.decode("utf-8"))
    except Exception:
        print(raw.decode("utf-8", errors="replace"))
        return 0 if 200 <= status < 300 else 1

    if not (200 <= status < 300):
        print(json.dumps(data, ensure_ascii=False, indent=2))
        return 1

    print()
    if args.raw_response:
        print(json.dumps(data, ensure_ascii=False, indent=2))
    elif args.mode == "embeddings":
        print_embeddings(data)
    elif args.mode == "rerank":
        print_rerank(data)
    elif args.mode == "asr":
        print_asr(data)
    elif args.mode == "messages":
        print_messages(data)
    else:
        print_chat(data)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except CliError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        raise SystemExit(2)
    except KeyboardInterrupt:
        print("Interrupted", file=sys.stderr)
        raise SystemExit(130)
