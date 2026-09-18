#!/usr/bin/env python3
"""Tiny BrighTO-Router smoke client.

Examples:
  python3 test_router.py --model my-model --text "Reply OK"
  python3 test_router.py --router http://SERVER:18080 --api-key lc-... --model my-model --text "Reply OK"
  python3 test_router.py --mode embeddings --model my-embedding --text "hello"
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
DEFAULT_DEMO_KEY = "lc-0123456789abcdef0123456789abcdef"


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

    if args.mode == "messages":
        if args.image or args.audio:
            raise CliError("messages mode in this helper is text-only; use --mode chat for OpenAI-style image/audio JSON")
        return "/v1/messages", {
            "model": args.model,
            "max_tokens": args.max_tokens or 128,
            "messages": [{"role": "user", "content": args.text}],
        }

    raise CliError(f"unknown mode: {args.mode}")


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
        description="Call BrighTO-Router once with a chat, embeddings, or Anthropic Messages request.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""Environment fallback order:
  router URL: BRIGHTO_ROUTER_URL, then BASE_URL from .env, then http://127.0.0.1:18080
  client key: BRIGHTO_ROUTER_API_KEY, ROUTER_API_KEY, BRIGHTO_API_KEY
  model:      BRIGHTO_MODEL

Provider keys such as OPENAI_API_KEY are intentionally ignored. This script tests the router as a client app.
""",
    )
    parser.add_argument("--router", help="Router base URL, for example http://127.0.0.1:18080")
    parser.add_argument("--api-key", help="BrighTO client API key, usually starts with lc-")
    parser.add_argument("--model", help="Public model route name in BrighTO-Router")
    parser.add_argument("--text", default="Reply OK in one short sentence.", help="Text input to send")
    parser.add_argument("--mode", choices=["chat", "embeddings", "messages"], default="chat", help="Request type")
    parser.add_argument("--image", action="append", default=[], help="Image file path, http URL, https URL, or data URL for OpenAI-style chat JSON")
    parser.add_argument("--audio", action="append", default=[], help="Audio file path for OpenAI-style chat JSON")
    parser.add_argument("--max-tokens", type=int, default=128, help="Max output tokens for chat/messages")
    parser.add_argument("--temperature", type=float, help="Optional chat temperature")
    parser.add_argument("--timeout", type=int, default=120, help="HTTP timeout seconds")
    parser.add_argument("--env-file", default=".env", help="Env file to read for local defaults")
    parser.add_argument("--insecure", action="store_true", help="Allow self-signed HTTPS certificates for local smoke tests")
    parser.add_argument("--raw-response", action="store_true", help="Print full JSON response")
    parser.add_argument("--dry-run", action="store_true", help="Print the request that would be sent, with media bytes redacted")
    args = parser.parse_args()

    env_file_values = parse_env_file(Path(args.env_file))
    router = (args.router or first_env(["BRIGHTO_ROUTER_URL", "BASE_URL"], env_file_values) or DEFAULT_ROUTER).rstrip("/")
    api_key = args.api_key or first_env(["BRIGHTO_ROUTER_API_KEY", "ROUTER_API_KEY", "BRIGHTO_API_KEY"], env_file_values)
    model = args.model or first_env(["BRIGHTO_MODEL"], env_file_values)
    if not api_key:
        raise CliError(
            "missing BrighTO client API key. Pass --api-key lc-... or set BRIGHTO_ROUTER_API_KEY in .env. "
            f"For a fresh local install, the demo key is {DEFAULT_DEMO_KEY}."
        )
    if api_key.startswith("sk-"):
        raise CliError("this looks like a provider key. Use a BrighTO client key from the Portal API Keys screen, usually lc-...")
    if not model:
        raise CliError("missing model. Pass --model <public-model-route> or set BRIGHTO_MODEL in .env")
    args.model = model

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
