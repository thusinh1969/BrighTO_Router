#!/usr/bin/env python3
"""
brighto.py — single-file Python SDK for BrighTO-Router. Python 3.9+, requests.

Public inference API coverage:

    ┌─ LLM (OpenAI-compatible)
    │     chat()      → /v1/chat/completions   (messages)
    │     complete()  → /v1/completions        (prompt)
    │     respond()   → /v1/responses          (input)
    │     stream()    → streamed chat text chunks
    │     messages()  → /v1/messages           (Anthropic dialect)
    ├─ Embedding      embed()      → /v1/embeddings
    ├─ Rerank         rerank()     → /v1/rerank
    ├─ ASR            transcribe() → /v1/audio/transcriptions (multipart)
    ├─ System One     systemone()  → /v1/systemone
    │                decisions()  → /v1/decisions
    └─ Meta           models()     → /v1/models

All public inference endpoints are supported. Admin/Portal APIs are separate.
Methods return friendly values; Reply.raw or post() preserves full JSON.
Use events() for raw streamed JSON, or stream() for chat text chunks.

Quick start:

    from brighto import Router
    c = Router(api_key="sk-brighto-...", model="my-chat-route")

    print(c.ask("2+2?"))                                        # chat
    c.embed("xin chào", model="qwen-embedding")                    # vector
    c.rerank("router speed", ["a", "b"], model="qwen-rerank")      # rank
    c.transcribe("audio.wav", model="whisper")                     # text
    c.systemone({"message": "..."}, {...}, model="my-decision")    # decision

Dependency: pip install requests

Optional environment defaults:
    export BRIGHTO_ROUTER_URL=http://127.0.0.1:18080
    export BRIGHTO_API_KEY=sk-brighto-...
    export BRIGHTO_MODEL=my-chat-route

BRIGHTO_BASE_URL is also accepted. Environment variables are read when the
client is constructed; .env is not loaded automatically. Model names are the
public route or Model Group names from the Portal, not provider model IDs.
"""

from __future__ import annotations

import json
import mimetypes
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, Iterator, List, Optional, Union
from urllib.parse import urlsplit

import requests


# ============================================================================
# Response types
# ============================================================================
@dataclass
class Reply:
    """One LLM response from chat, completions, Responses, or Messages."""

    content: str = ""
    reasoning: str = ""
    tool_calls: List[Dict[str, Any]] = field(default_factory=list)
    usage: Dict[str, Any] = field(default_factory=dict)
    finish_reason: str = ""
    raw: Dict[str, Any] = field(default_factory=dict)

    def __str__(self) -> str:
        return self.content

    @property
    def text(self) -> str:
        """Answer text, excluding reasoning."""
        return self.content

    @property
    def called_tool(self) -> bool:
        """Whether the model returned tool calls for the application to execute."""
        return bool(self.tool_calls)


class RouterError(Exception):
    """An HTTP, network, streaming, or response error."""

    def __init__(self, message: str, status_code: Optional[int] = None, request_id: Optional[str] = None):
        super().__init__(message)
        self.status_code = status_code
        self.request_id = request_id


# ============================================================================
# Client
# ============================================================================
class Router:
    """Client for one BrighTO-Router endpoint.

    Every inference method accepts ``model=`` to override the client default.
    """

    DEFAULT_BASE_URL = "http://127.0.0.1:18080/v1"

    def __init__(
        self,
        base_url: Optional[str] = None,
        api_key: Optional[str] = None,
        model: Optional[str] = None,
        timeout: float = 300,
        verify: Union[bool, str] = True,
    ):
        url = base_url if base_url is not None else (
            os.getenv("BRIGHTO_ROUTER_URL") or os.getenv("BRIGHTO_BASE_URL") or self.DEFAULT_BASE_URL
        )
        self.base_url = self._base_url(url)
        key = api_key if api_key is not None else os.getenv("BRIGHTO_API_KEY", "")
        self.api_key = key.strip()
        self.model = model if model is not None else os.getenv("BRIGHTO_MODEL")
        self.timeout = timeout
        if not self.api_key or not all(33 <= ord(c) <= 126 for c in self.api_key):
            raise ValueError("Use a BrighTO client API key from the Portal or BRIGHTO_API_KEY; it must contain only printable ASCII without spaces.")
        if timeout <= 0:
            raise ValueError("timeout must be greater than zero.")
        self._session = requests.Session()
        self._session.headers.update({"Authorization": f"Bearer {self.api_key}"})
        self._session.verify = verify
        self.last_status_code: Optional[int] = None
        self.last_headers: Dict[str, str] = {}

    # ---------------------------------------------------------------- helpers
    @staticmethod
    def _base_url(value: str) -> str:
        parsed = urlsplit(value.strip())
        if (parsed.scheme not in ("http", "https") or not parsed.hostname
                or parsed.username is not None or parsed.password is not None
                or parsed.query or parsed.fragment):
            raise ValueError("base_url must be an HTTP(S) router URL without credentials, a query, or a fragment.")
        path = parsed.path.rstrip("/")
        while path.endswith("/v1"):
            path = path[:-3].rstrip("/")
        if any(part in ("v1", ".", "..") for part in path.split("/")):
            raise ValueError("Use the router root or /v1 as base_url, not an API endpoint.")
        return f"{parsed.scheme}://{parsed.netloc}{path}/v1"

    def _url(self, path: str) -> str:
        path = path.lstrip("/")
        while path.startswith("v1/"):
            path = path[3:]
        if not path or any(c in path for c in (":", "?", "#", "\\")) or any(p in (".", "..") for p in path.split("/")):
            raise ValueError("Use a relative API path such as /embeddings or /v1/embeddings.")
        return f"{self.base_url}/{path}"

    def _payload(self, payload: Dict[str, Any]) -> Dict[str, Any]:
        result = dict(payload)
        model = result.get("model")
        if model is None:
            model = self.model
        if not isinstance(model, str) or not model.strip():
            raise ValueError("Set model to the public API model name of a route or Model Group, or set BRIGHTO_MODEL.")
        result["model"] = model
        return result

    def _request(self, method: str, path: str, **kwargs: Any) -> requests.Response:
        self.last_status_code = None
        self.last_headers = {}
        try:
            response = self._session.request(method, self._url(path), timeout=self.timeout, **kwargs)
        except requests.RequestException as exc:
            raise RouterError(f"Request failed ({type(exc).__name__}); check the router URL, TLS certificate, and timeout.") from exc
        self.last_status_code = response.status_code
        self.last_headers = dict(response.headers)
        if not 200 <= response.status_code < 300:
            try:
                request_id = response.headers.get("x-router-request-id")
                message = f"HTTP {response.status_code}: {response.text[:600]}"
                if request_id:
                    message += f" (request_id={request_id})"
                raise RouterError(message, response.status_code, request_id)
            finally:
                response.close()
        return response

    @staticmethod
    def _json(response: requests.Response) -> Dict[str, Any]:
        try:
            data = response.json()
        except ValueError as exc:
            raise RouterError("Expected a JSON object from the router.", response.status_code, response.headers.get("x-router-request-id")) from exc
        if not isinstance(data, dict):
            raise RouterError("Expected a JSON object from the router.", response.status_code, response.headers.get("x-router-request-id"))
        if data.get("error") or data.get("type") == "error":
            raise RouterError(f"Upstream error: {json.dumps(data, ensure_ascii=False)[:600]}", response.status_code, response.headers.get("x-router-request-id"))
        return data

    def post(self, path: str, payload: Optional[Dict[str, Any]] = None, **fields: Any) -> Dict[str, Any]:
        """Return full JSON from an API path, forwarding provider options unchanged."""
        body = self._payload(dict(payload or {}, **fields))
        if body.get("stream"):
            raise ValueError("Use stream() or events() for streaming requests.")
        with self._request("POST", path, json=body) as response:
            return self._json(response)

    def events(self, path: str, payload: Optional[Dict[str, Any]] = None, **fields: Any) -> Iterator[Dict[str, Any]]:
        """Yield JSON Server-Sent Events (SSE), including tools, reasoning, and usage.

        Consume fully, or close the iterator when stopping early.
        """
        body = self._payload(dict(payload or {}, **fields))
        body["stream"] = True
        with self._request("POST", path, json=body, stream=True) as response:
            if "text/event-stream" not in response.headers.get("content-type", "").lower():
                raise RouterError("Expected a Server-Sent Events response; check that this backend supports streaming.", response.status_code, response.headers.get("x-router-request-id"))
            data_lines: List[str] = []
            try:
                # Frame on wire newlines before UTF-8 decoding. Unicode line
                # separators inside JSON strings are content, not SSE frames.
                for raw_line in response.iter_lines(decode_unicode=False):
                    try:
                        line = raw_line.decode("utf-8")
                    except UnicodeDecodeError as exc:
                        raise RouterError("Invalid UTF-8 in a streaming event.", response.status_code, response.headers.get("x-router-request-id")) from exc
                    if line.startswith("data:"):
                        data_lines.append(line[5:].lstrip(" "))
                    elif not line and data_lines:
                        data = "\n".join(data_lines)
                        data_lines.clear()
                        if data.strip() == "[DONE]":
                            return
                        try:
                            chunk = json.loads(data)
                        except ValueError as exc:
                            raise RouterError("Invalid JSON in a streaming event.", response.status_code, response.headers.get("x-router-request-id")) from exc
                        if not isinstance(chunk, dict):
                            raise RouterError("Expected a JSON object in a streaming event.")
                        if chunk.get("error") or chunk.get("type") == "error":
                            raise RouterError(f"Streaming error: {json.dumps(chunk, ensure_ascii=False)[:600]}", response.status_code, response.headers.get("x-router-request-id"))
                        yield chunk
                if data_lines:
                    raise RouterError("The stream ended during an incomplete event.")
            except requests.RequestException as exc:
                raise RouterError(f"Stream interrupted ({type(exc).__name__}).", response.status_code, response.headers.get("x-router-request-id")) from exc

    @staticmethod
    def _to_reply(data: Dict[str, Any]) -> Reply:
        choices = data.get("choices")
        if not isinstance(choices, list) or not choices or not isinstance(choices[0], dict) or not isinstance(choices[0].get("message"), dict):
            raise RouterError("Expected a Chat Completions response; check the route type or use the matching SDK method.")
        choice = choices[0]
        msg = choice["message"]
        return Reply(
            content=msg.get("content") or "",
            reasoning=msg.get("reasoning_content") or msg.get("reasoning") or "",
            tool_calls=msg.get("tool_calls") or [],
            usage=data.get("usage") or {},
            finish_reason=choice.get("finish_reason") or "",
            raw=data,
        )

    def _chat_kwargs(
        self,
        *,
        model: Optional[str],
        max_tokens: int,
        temperature: Optional[float],
        top_p: Optional[float],
        thinking: Optional[bool],
        reasoning_effort: Optional[str],
    ) -> Dict[str, Any]:
        """Provider-neutral defaults; provider-specific thinking is opt-in."""
        out: Dict[str, Any] = {
            "model": model,
            "max_tokens": max_tokens,
            "stream": False,
        }
        if thinking is not None:
            out["chat_template_kwargs"] = {"enable_thinking": thinking}
        if reasoning_effort is not None:
            out["reasoning_effort"] = reasoning_effort
        if temperature is not None:
            out["temperature"] = temperature
        if top_p is not None:
            out["top_p"] = top_p
        return out

    # ============================================================ LLM — chat
    def chat(
        self,
        messages: List[Dict[str, Any]],
        *,
        model: Optional[str] = None,
        system: Optional[str] = None,
        max_tokens: int = 512,
        temperature: Optional[float] = None,
        top_p: Optional[float] = None,
        thinking: Optional[bool] = None,
        reasoning_effort: Optional[str] = None,
        tools: Optional[List[Dict[str, Any]]] = None,
        **options: Any,
    ) -> Reply:
        """Chat Completions using OpenAI-compatible messages.

        Parameters
        ----------
        messages : list[dict]
            Messages: ``{"role": "system|user|assistant|tool", "content": ...}``.
        model : str | None
            Public route or Model Group API name, overriding the client default.
        system : str | None
            Prepend a system message when supplied.
        thinking : bool | None
            Optional chat_template_kwargs.enable_thinking for compatible models.
            Nothing provider-specific is sent by default.
        reasoning_effort : str | None
            Top-level reasoning_effort; supported values depend on the provider.
        tools : list[dict] | None
            OpenAI-compatible tool definitions. Returned calls are available
            in ``Reply.tool_calls``.

        Returns
        -------
        Reply
        """
        if system is not None:
            messages = [{"role": "system", "content": system}] + list(messages)
        payload = self._chat_kwargs(
            model=model,
            max_tokens=max_tokens,
            temperature=temperature,
            top_p=top_p,
            thinking=thinking,
            reasoning_effort=reasoning_effort,
        )
        payload["messages"] = messages
        if tools is not None:
            payload["tools"] = tools
        payload.update(options)
        if thinking is not None:
            payload["chat_template_kwargs"] = dict(payload.get("chat_template_kwargs") or {}, enable_thinking=thinking)
        return self._to_reply(self.post("/chat/completions", payload))

    def ask(
        self,
        text: str,
        *,
        model: Optional[str] = None,
        system: Optional[str] = None,
        **kwargs,
    ) -> Reply:
        """Send one user message and return a Reply."""
        return self.chat([{"role": "user", "content": text}], model=model, system=system, **kwargs)

    def stream(
        self,
        text: Union[str, List[Dict[str, Any]]],
        *,
        model: Optional[str] = None,
        system: Optional[str] = None,
        thinking: Optional[bool] = None,
        **kwargs,
    ) -> Iterator[str]:
        """Yield chat text chunks; accepts text or conversation messages."""
        messages = [{"role": "user", "content": text}] if isinstance(text, str) else list(text)
        if system is not None:
            messages.insert(0, {"role": "system", "content": system})
        payload: Dict[str, Any] = {
            "model": model,
            "messages": messages,
            "max_tokens": kwargs.pop("max_tokens", 512),
            "stream": True,
        }
        payload.update(kwargs)
        if thinking is not None:
            payload["chat_template_kwargs"] = dict(payload.get("chat_template_kwargs") or {}, enable_thinking=thinking)
        events = self.events("/chat/completions", payload)
        try:
            for chunk in events:
                choices = chunk.get("choices") or []
                if not choices:
                    continue
                piece = (choices[0].get("delta") or {}).get("content")
                if piece:
                    yield piece
        finally:
            events.close()

    # ====================================================== LLM — completions
    def complete(
        self,
        prompt: str,
        *,
        model: Optional[str] = None,
        max_tokens: int = 512,
        temperature: Optional[float] = None,
        **options: Any,
    ) -> Reply:
        """Legacy Completions with a plain prompt."""
        payload: Dict[str, Any] = {
            "model": model,
            "prompt": prompt,
            "max_tokens": max_tokens,
        }
        if temperature is not None:
            payload["temperature"] = temperature
        payload.update(options)
        data = self.post("/completions", payload)
        choices = data.get("choices")
        if not isinstance(choices, list) or not choices or not isinstance(choices[0], dict) or "text" not in choices[0]:
            raise RouterError("Expected a Completions response with choices[].text.")
        choice = choices[0]
        return Reply(
            content=choice.get("text") or "",
            usage=data.get("usage") or {},
            finish_reason=choice.get("finish_reason") or "",
            raw=data,
        )

    # ======================================================= LLM — responses
    def respond(
        self,
        input_text: Any,
        *,
        model: Optional[str] = None,
        max_tokens: int = 512,
        instructions: Optional[str] = None,
        **options: Any,
    ) -> Reply:
        """Responses API; accepts text or structured input, preserving full data in raw."""
        payload: Dict[str, Any] = {
            "model": model,
            "input": input_text,
            "max_output_tokens": max_tokens,
        }
        if instructions is not None:
            payload["instructions"] = instructions
        payload.update(options)
        data = self.post("/responses", payload)
        if "output" not in data and "output_text" not in data:
            raise RouterError("Expected a Responses API response with output or output_text.")
        # Responses output is a list of message, reasoning, and tool items.
        parts = [p for p in (data.get("output") or []) if p.get("type") == "message"]
        content = ""
        for p in parts:
            for c in p.get("content") or []:
                if c.get("type") in ("output_text", "text"):
                    content += c.get("text", "")
        return Reply(
            content=content or data.get("output_text") or "",
            reasoning="".join(c.get("text", "") for p in (data.get("output") or []) if p.get("type") == "reasoning" for c in (p.get("summary") or [])),
            tool_calls=[p for p in (data.get("output") or []) if p.get("type") == "function_call"],
            usage=data.get("usage") or {},
            finish_reason=data.get("status") or "",
            raw=data,
        )

    # ================================================ LLM — Anthropic dialect
    def messages(
        self,
        messages: List[Dict[str, Any]],
        *,
        model: Optional[str] = None,
        max_tokens: int = 512,
        system: Optional[str] = None,
        **options: Any,
    ) -> Reply:
        """Anthropic Messages format, including tools and thinking in Reply/raw."""
        payload: Dict[str, Any] = {
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
        }
        if system is not None:
            payload["system"] = system
        payload.update(options)
        data = self.post("/messages", payload)
        if not isinstance(data.get("content"), list):
            raise RouterError("Expected an Anthropic Messages response with content blocks.")
        content = "".join(
            (c.get("text") or "") for c in (data.get("content") or []) if c.get("type") == "text"
        )
        return Reply(
            content=content,
            reasoning="".join(c.get("thinking", "") for c in data["content"] if c.get("type") == "thinking"),
            tool_calls=[c for c in data["content"] if c.get("type") == "tool_use"],
            usage=data.get("usage") or {},
            finish_reason=data.get("stop_reason") or "",
            raw=data,
        )

    # ============================================================= embedding
    def embed(
        self,
        input_text: Union[str, List[str]],
        *,
        model: Optional[str] = None,
        **options: Any,
    ) -> Union[List[float], List[List[float]]]:
        """Return float embedding vectors in input order.

        A string returns ``list[float]``; a list of strings returns
        ``list[list[float]]``. Use post() for base64 or multimodal formats.
        """
        if not isinstance(input_text, (str, list)) or (isinstance(input_text, list) and (not input_text or not all(isinstance(item, str) for item in input_text))):
            raise ValueError("embed() accepts text or a nonempty list of text strings; use post('/embeddings', ...) for other input shapes.")
        if options.get("encoding_format", "float") != "float":
            raise ValueError("embed() returns float vectors; use post('/embeddings', ...) for base64 encoding.")
        payload = {"model": model, "input": input_text}
        payload.update(options)
        data = self.post("/embeddings", payload)
        rows = data.get("data")
        count = 1 if isinstance(input_text, str) else len(input_text)
        if not isinstance(rows, list) or len(rows) != count or any(not isinstance(row, dict) or not isinstance(row.get("embedding"), list) for row in rows):
            raise RouterError("Expected one float embedding vector for each input.")
        if any("index" in row for row in rows):
            indices = [row.get("index") for row in rows]
            if any(type(index) is not int for index in indices) or sorted(indices) != list(range(count)):
                raise RouterError("Embedding indices must identify every input exactly once.")
            rows = sorted(rows, key=lambda row: row["index"])
        vecs = [row["embedding"] for row in rows]
        return vecs[0] if isinstance(input_text, str) else vecs

    # ================================================================ rerank
    def rerank(
        self,
        query: str,
        documents: List[str],
        *,
        model: Optional[str] = None,
        top_n: Optional[int] = None,
        **options: Any,
    ) -> Dict[str, Any]:
        """Rank documents against a query, returning full JSON.

        Returns
        -------
        dict
            Full provider response, commonly ``results`` with each document's
            original ``index`` and ``relevance_score``.
        """
        payload: Dict[str, Any] = {
            "model": model,
            "query": query,
            "documents": documents,
        }
        if top_n is not None:
            payload["top_n"] = top_n
        payload.update(options)
        return self.post("/rerank", payload)

    # =================================================================== ASR
    def transcribe(
        self,
        file_path: Union[str, Path],
        *,
        model: Optional[str] = None,
        language: Optional[str] = None,
        **options: Any,
    ) -> str:
        """Transcribe audio with a multipart upload, returning text.

        Parameters
        ----------
        file_path : str | Path
            Audio file path, such as WAV or MP3.
        model : str | None
            Model transcription (OpenAI-compatible).
        language : str | None
            Optional language code, such as ``"vi"``.

        Returns
        -------
        str
            Transcribed text (``response["text"]``).
        """
        fields = dict(options, model=model)
        if language is not None:
            fields["language"] = language
        return self.transcription(file_path, **fields)["text"]

    def transcription(self, file_path: Union[str, Path], **options: Any) -> Dict[str, Any]:
        """Upload audio and return full JSON; text/srt/vtt responses use {"text": ...}."""
        path = Path(file_path)
        fields = self._payload(options)
        if fields.get("stream"):
            raise ValueError("Streaming transcription is not supported by this client.")
        content_type = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        with path.open("rb") as audio:
            with self._request("POST", "/audio/transcriptions", data=fields,
                               files={"file": (path.name, audio, content_type)}) as response:
                if fields.get("response_format") in ("text", "srt", "vtt"):
                    response.encoding = "utf-8"
                    return {"text": response.text}
                data = self._json(response)
                if not isinstance(data.get("text"), str):
                    raise RouterError("Expected a transcription response with text.")
                return data

    # ========================================================== System One
    def systemone(
        self,
        state: Any,
        questions: Dict[str, Any],
        *,
        model: Optional[str] = None,
        **options: Any,
    ) -> Dict[str, Any]:
        """System One / Decision — typed questions against a supplied state.

        Parameters
        ----------
        state : str, dict, or list
            Input state, such as ``{"message": "Charged twice."}``.
        questions : dict
            Typed decision questions::

                {
                  "duplicate_charge": {"type": "noul",
                                       "instructions": "Was the customer charged twice?"},
                  "team": {"type": "choice",
                           "instructions": "Which team should handle this?",
                           "criteria": {"billing": "payments",
                                        "support": "technical support"}},
                }

        Returns
        -------
        dict
            Full response, including ``answers``, usage, and warnings.
        """
        return self.post("/systemone", model=model, state=state, questions=questions, **options)

    # Both API aliases use the same System One route type.
    def decisions(self, state: Any, questions: Dict[str, Any], *, model: Optional[str] = None, **options: Any) -> Dict[str, Any]:
        """The same decision contract through the /v1/decisions endpoint alias."""
        return self.post("/decisions", model=model, state=state, questions=questions, **options)

    # ================================================================ meta
    def models(self) -> List[str]:
        """List public model and Model Group names available to this key."""
        with self._request("GET", "/models") as response:
            return [m["id"] for m in self._json(response).get("data", []) if isinstance(m, dict) and isinstance(m.get("id"), str)]

    def close(self) -> None:
        self._session.close()

    def __enter__(self) -> Router:
        return self

    def __exit__(self, *args: Any) -> None:
        self.close()

    def __repr__(self) -> str:
        return f"Router(base_url={self.base_url!r}, model={self.model!r})"


# ============================================================================
# Quick CLI: python brighto.py --model <public-chat-route> "Reply OK"
# ============================================================================
if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Call a BrighTO chat route using the single-file SDK.")
    parser.add_argument("text", nargs="?", default="Reply OK in one short sentence.")
    parser.add_argument("--router", help="Router root URL or URL ending in /v1")
    parser.add_argument("--model", help="Public route or Model Group name; defaults to BRIGHTO_MODEL")
    parser.add_argument("--stream", action="store_true")
    parser.add_argument("--ca", help="Path to a trusted CA/certificate PEM file")
    args = parser.parse_args()
    try:
        with Router(base_url=args.router, model=args.model, verify=args.ca or True) as client:
            if args.stream:
                for piece in client.stream(args.text):
                    print(piece, end="", flush=True)
                print()
            else:
                print(client.ask(args.text))
    except (ValueError, RouterError) as exc:
        parser.exit(1, f"{exc}\n")
