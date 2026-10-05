"""Python SDK contract tests. Run: python3 -m unittest discover -s tests -p test_python_sdk.py -v"""
import json
import os
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest.mock import patch

import requests

from brighto import Router, RouterError


class FixtureHandler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        self.respond()

    def do_POST(self):
        self.respond()

    def respond(self):
        self.server.calls.append((self.command, self.path, dict(self.headers), self.rfile.read(int(self.headers.get("Content-Length", "0")))))
        status, content_type, body = self.server.reply
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("x-router-request-id", "sdk-test-id")
        self.end_headers()
        self.wfile.write(body)


class SDKTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), FixtureHandler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.url = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.env = patch.dict(os.environ, {}, clear=True)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.server.calls = []
        self.reply({"choices": [{"message": {"content": " OK\n"}, "finish_reason": "stop"}], "usage": {"total_tokens": 2}})
        self.client = Router(self.url, "sk-brighto-test", "public-chat")
        self.addCleanup(self.client.close)

    def reply(self, data, status=200, content_type="application/json"):
        body = data if isinstance(data, bytes) else json.dumps(data, ensure_ascii=False).encode()
        self.server.reply = (status, content_type, body)

    def body(self):
        return json.loads(self.server.calls[-1][3])

    def test_chat_default_payload_is_provider_neutral(self):
        answer = self.client.ask("Xin chào", temperature=0, top_p=0)
        self.assertEqual(str(answer), " OK\n")
        self.assertEqual(answer.text, " OK\n")
        self.assertEqual(answer.usage, {"total_tokens": 2})
        self.assertEqual(answer.finish_reason, "stop")
        self.assertEqual(self.body(), {"model": "public-chat", "messages": [{"role": "user", "content": "Xin chào"}], "max_tokens": 512, "stream": False, "temperature": 0, "top_p": 0})
        self.assertEqual(self.server.calls[-1][1], "/v1/chat/completions")
        self.assertEqual(self.server.calls[-1][2]["Authorization"], "Bearer sk-brighto-test")

    def test_url_accepts_root_v1_and_duplicate_v1(self):
        for suffix in ("", "/", "/v1", "/v1/", "/v1/v1/"):
            with self.subTest(suffix=suffix), Router(self.url + suffix, "sk-test", "chat") as client:
                client.ask("OK")
                self.assertEqual(self.server.calls[-1][1], "/v1/chat/completions")
        for path in ("embeddings", "/embeddings", "/v1/embeddings", "/v1/v1/embeddings"):
            self.client.post(path, input="text")
            self.assertEqual(self.server.calls[-1][1], "/v1/embeddings")

    def test_url_rejects_credentials_full_endpoint_and_unsafe_paths(self):
        for url in ("ftp://example.com", "https://user:secret@example.com", self.url + "/v1/chat/completions", self.url + "?key=secret", self.url + "/../v1"):
            with self.subTest(url=url), self.assertRaises(ValueError):
                Router(url, "sk-test")
        for path in ("https://example.com/embeddings", "../embeddings", "embeddings?key=secret", ""):
            with self.subTest(path=path), self.assertRaises(ValueError):
                self.client.post(path, input="x")
        self.assertEqual(self.server.calls, [])

    def test_environment_is_read_at_client_creation(self):
        with patch.dict(os.environ, BRIGHTO_BASE_URL=self.url, BRIGHTO_API_KEY="sk-env", BRIGHTO_MODEL="group-env"):
            with Router() as client:
                client.ask("x")
                self.assertEqual(self.body()["model"], "group-env")
            with patch.dict(os.environ, BRIGHTO_ROUTER_URL=self.url + "/v1"):
                with Router() as client:
                    self.assertEqual(client.base_url, self.url + "/v1")

    def test_default_url_is_the_router_install_port(self):
        with Router(api_key="sk-test") as client:
            self.assertEqual(client.base_url, "http://127.0.0.1:18080/v1")
            self.assertIsNone(client.model)

    def test_missing_model_has_clear_error_but_models_still_works(self):
        self.reply({"data": [{"id": "chat"}, {"id": "group"}]})
        with Router(self.url, "sk-test") as client:
            self.assertEqual(client.models(), ["chat", "group"])
            with self.assertRaisesRegex(ValueError, "public API model name"):
                client.ask("OK")

    def test_bad_key_is_rejected_before_network_without_leaking_it(self):
        for key in ("", "sk-\u200btest", "sk–test", "sk test", "sk\ntest", "sk\ttest", "sk-中文"):
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "printable ASCII"):
                Router(self.url, key)
        self.assertEqual(self.server.calls, [])
        with Router(self.url, "  sk-clean\n", "chat") as client:
            client.ask("OK")
            self.assertEqual(self.server.calls[-1][2]["Authorization"], "Bearer sk-clean")
            self.assertNotIn("sk-clean", repr(client))

    def test_reasoning_tools_and_extra_fields(self):
        tools = [{"type": "function", "function": {"name": "weather", "parameters": {"type": "object"}}}]
        calls = [{"id": "call_1", "type": "function", "function": {"name": "weather", "arguments": "{}"}}]
        self.reply({"choices": [{"message": {"content": None, "reasoning_content": " think\n", "tool_calls": calls}, "finish_reason": "tool_calls"}], "extra": 42})
        answer = self.client.ask("weather", thinking=True, reasoning_effort="medium", tools=tools, tool_choice="auto", chat_template_kwargs={"custom": 1})
        self.assertTrue(answer.called_tool)
        self.assertEqual(answer.tool_calls, calls)
        self.assertEqual(answer.reasoning, " think\n")
        self.assertEqual(answer.raw["extra"], 42)
        body = self.body()
        self.assertEqual(body["reasoning_effort"], "medium")
        self.assertEqual(body["chat_template_kwargs"], {"custom": 1, "enable_thinking": True})
        self.assertEqual(body["tool_choice"], "auto")
        self.assertEqual(body["tools"], tools)

    def test_multi_turn_multimodal_input_and_override_are_preserved(self):
        messages = [{"role": "user", "content": [{"type": "image_url", "image_url": {"url": "data:image/png;base64,abc"}}, {"type": "input_audio", "input_audio": {"data": "abc", "format": "wav"}}]}, {"role": "assistant", "content": "prior"}]
        before = json.dumps(messages)
        self.client.chat(messages, system="instructions", model="vision-group")
        self.assertEqual(self.body()["messages"], [{"role": "system", "content": "instructions"}] + messages)
        self.assertEqual(self.body()["model"], "vision-group")
        self.assertEqual(json.dumps(messages), before)

    def test_each_api_helper_uses_its_own_endpoint_and_full_json(self):
        cases = [
            ("completions", lambda: self.client.complete("prompt", model="completion").raw, {"prompt": "prompt", "model": "completion", "max_tokens": 512}, {"choices": [{"text": "OK"}], "extra": 1}),
            ("responses", lambda: self.client.respond("prompt", model="responses").raw, {"input": "prompt", "model": "responses", "max_output_tokens": 512}, {"output": [], "extra": 2}),
            ("messages", lambda: self.client.messages([{"role": "user", "content": "x"}], model="anthropic", max_tokens=64).raw, {"messages": [{"role": "user", "content": "x"}], "max_tokens": 64, "model": "anthropic"}, {"content": [], "extra": 3}),
            ("rerank", lambda: self.client.rerank("query", ["doc1", "doc2"], model="rank", top_n=1), {"query": "query", "documents": ["doc1", "doc2"], "model": "rank", "top_n": 1}, {"results": [{"index": 0, "relevance_score": 0.9}], "extra": 4}),
            ("systemone", lambda: self.client.systemone({"x": 1}, {"flag": {"type": "noul"}}, model="decision"), {"state": {"x": 1}, "questions": {"flag": {"type": "noul"}}, "model": "decision"}, {"answers": {"flag": {"noul": 0.9}}, "extra": 5}),
            ("decisions", lambda: self.client.decisions({"x": 1}, {"flag": {"type": "noul"}}, model="decision"), {"state": {"x": 1}, "questions": {"flag": {"type": "noul"}}, "model": "decision"}, {"answers": {"flag": {"noul": 0.9}}, "extra": 5}),
        ]
        for path, call, body, response in cases:
            with self.subTest(path=path):
                self.reply(response)
                self.assertEqual(call(), response)
                self.assertEqual(self.server.calls[-1][1], "/v1/" + path)
                self.assertEqual(self.body(), body)

    def test_multipart_upload_and_text_response(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "sample.wav"
            file.write_bytes(b"RIFF-audio-fixture")
            self.reply({"text": "transcribed"})
            self.assertEqual(self.client.transcribe(file, model="asr", language="en"), "transcribed")
            _, path, headers, raw = self.server.calls[-1]
            self.assertEqual(path, "/v1/audio/transcriptions")
            self.assertIn("multipart/form-data; boundary=", headers["Content-Type"])
            for value in (b'filename="sample.wav"', b"RIFF-audio-fixture", b"\r\n\r\nasr\r\n", b"\r\n\r\nen\r\n"):
                self.assertIn(value, raw)
            self.reply("Xin chào".encode(), content_type="text/plain")
            self.assertEqual(self.client.transcribe(file, response_format="text"), "Xin chào")

    def test_stream_utf8_comments_usage_and_done(self):
        self.reply((
            ': keepalive\n\nevent: message\ndata: {"choices":[{"delta":{"content":"Xin chào 🌟"}}]}\n\n'
            'data: {"choices": [], "usage": {"total_tokens": 5}}\n\n'
            'data: {"choices":[{"delta":{"content":"!"}}]}\n\ndata: [DONE]\n\n'
        ).encode(), content_type="text/event-stream")
        self.assertEqual("".join(self.client.stream("hello", thinking=False)), "Xin chào 🌟!")
        self.assertEqual(self.body()["chat_template_kwargs"], {"enable_thinking": False})
        self.assertTrue(self.body()["stream"])

    def test_events_preserve_raw_data_and_support_multiline_sse(self):
        self.reply(b'data: {"type": "response.completed",\ndata: "usage": {"total_tokens": 3}}\n\ndata: [DONE]\n\n', content_type="text/event-stream")
        self.assertEqual(list(self.client.events("responses", input="x", stream=True)), [{"type": "response.completed", "usage": {"total_tokens": 3}}])
        self.assertEqual(self.server.calls[-1][1], "/v1/responses")

    def test_stream_unicode_line_separators_are_content_not_frames(self):
        content = "one\u2028two\u2029three\u0085four"
        event = {"choices": [{"delta": {"content": content}}]}
        self.reply(("data: " + json.dumps(event, ensure_ascii=False) + "\r\n\r\ndata: [DONE]\r\n\r\n").encode(), content_type="text/event-stream")
        self.assertEqual("".join(self.client.stream("hello")), content)

    def test_stream_invalid_utf8_has_a_clear_error(self):
        self.reply(b'data: {"value":"\xff"}\n\n', content_type="text/event-stream")
        with self.assertRaisesRegex(RouterError, "Invalid UTF-8"):
            list(self.client.events("responses", input="hello"))

    def test_stream_accepts_multi_turn_messages(self):
        self.reply(b'data: {"choices":[{"delta":{"content":"OK"}}]}\n\ndata: [DONE]\n\n', content_type="text/event-stream")
        messages = [{"role": "user", "content": "hello"}, {"role": "assistant", "content": "hi"}, {"role": "user", "content": "again"}]
        self.assertEqual("".join(self.client.stream(messages, system="be brief")), "OK")
        self.assertEqual(self.body()["messages"], [{"role": "system", "content": "be brief"}] + messages)
        self.assertNotIn("chat_template_kwargs", self.body())

    def test_stream_errors_are_never_silently_swallowed(self):
        for raw, message in [(b'data: {"error":{"message":"quota exceeded"}}\n\n', "quota exceeded"), (b"data: invalid-json\n\n", "Invalid JSON"), (b'data: {"choices": []}', "incomplete event")]:
            with self.subTest(raw=raw):
                self.reply(raw, content_type="text/event-stream")
                with self.assertRaisesRegex(RouterError, message):
                    list(self.client.stream("hello"))

    def test_stream_rejects_a_nonstreaming_response(self):
        with self.assertRaisesRegex(RouterError, "supports streaming"):
            list(self.client.stream("hello"))

    def test_early_stream_close_releases_response(self):
        self.reply(b'data: {"choices":[{"delta":{"content":"OK"}}]}\n\ndata: [DONE]\n\n', content_type="text/event-stream")
        response = None
        original = self.client._request
        def record(*args, **kwargs):
            nonlocal response
            response = original(*args, **kwargs)
            return response
        with patch.object(self.client, "_request", side_effect=record):
            stream = self.client.stream("hello")
            self.assertEqual(next(stream), "OK")
            stream.close()
            self.assertTrue(response.raw.closed)

    def test_http_errors_keep_status_detail_and_request_id(self):
        for status in (401, 403, 404, 429, 503):
            with self.subTest(status=status):
                self.reply(b"no healthy backend: circuit-open", status=status, content_type="text/plain")
                with self.assertRaises(RouterError) as caught:
                    self.client.ask("hello")
                self.assertEqual(caught.exception.status_code, status)
                self.assertEqual(caught.exception.request_id, "sdk-test-id")
                self.assertIn("circuit-open", str(caught.exception))

    def test_network_errors_are_wrapped_without_automatic_retry(self):
        for error in (requests.Timeout, requests.ConnectionError, requests.exceptions.SSLError):
            with self.subTest(error=error), patch.object(self.client._session, "request", side_effect=error("test")) as send:
                with self.assertRaisesRegex(RouterError, error.__name__):
                    self.client.ask("x")
                self.assertEqual(send.call_count, 1)

    def test_malformed_success_and_wrong_chat_shape_are_errors(self):
        for raw in (b"not json", b"[]", b'{"error":{"message":"bad request"}}', b'{"output":[]}'):
            with self.subTest(raw=raw):
                self.reply(raw)
                with self.assertRaises(RouterError):
                    self.client.ask("x")

    def test_nonstream_method_rejects_stream_true(self):
        with self.assertRaisesRegex(ValueError, "events"):
            self.client.respond("x", stream=True)
        self.assertEqual(self.server.calls, [])

    def test_tls_verification_and_context_manager_cleanup(self):
        with Router(self.url, "sk-test", verify="private-ca.pem") as client:
            self.assertEqual(client._session.verify, "private-ca.pem")
            with patch.object(client._session, "close") as close:
                client.__exit__(None, None, None)
                close.assert_called_once()
        self.assertTrue(self.client._session.verify)

    def test_generic_post_does_not_mutate_input(self):
        payload = {"model": "decision", "state": {"x": 1}, "questions": {}}
        original = json.dumps(payload)
        self.client.post("/v1/decisions", payload)
        self.assertEqual(json.dumps(payload), original)
        self.assertEqual(self.server.calls[-1][1], "/v1/decisions")

    def test_embedding_single_and_batch_follow_input_order(self):
        self.reply({"data": [{"index": 0, "embedding": [0.125, -0.25]}]})
        self.assertEqual(self.client.embed("a", model="embed", dimensions=2), [0.125, -0.25])
        self.assertEqual(self.body(), {"model": "embed", "input": "a", "dimensions": 2})
        self.reply({"data": [{"index": 1, "embedding": [2.0]}, {"index": 0, "embedding": [1.0]}]})
        self.assertEqual(self.client.embed(["a", "b"]), [[1.0], [2.0]])

    def test_embedding_bad_shape_indices_and_encoding_are_clear_errors(self):
        for data in ({"data": []}, {"data": [{"embedding": None}]}, {"data": [{"index": 1, "embedding": [1.0]}]}, {"data": [{"index": 0, "embedding": "base64-data"}]}):
            self.reply(data)
            with self.subTest(data=data), self.assertRaises(RouterError):
                self.client.embed("a")
        with self.assertRaises(ValueError):
            self.client.embed("a", encoding_format="base64")
        with self.assertRaises(ValueError):
            self.client.embed([])

    def test_responses_extract_text_reasoning_and_function_calls(self):
        function = {"type": "function_call", "name": "weather", "arguments": "{}", "call_id": "a"}
        self.reply({"output": [{"type": "reasoning", "summary": [{"type": "summary_text", "text": "thinking"}]}, {"type": "message", "content": [{"type": "output_text", "text": " OK\n"}]}, function], "status": "completed"})
        answer = self.client.respond("x", instructions="be brief", previous_response_id="response_1")
        self.assertEqual(answer.content, " OK\n")
        self.assertEqual(answer.reasoning, "thinking")
        self.assertEqual(answer.tool_calls, [function])
        self.assertEqual(answer.finish_reason, "completed")
        self.assertEqual(self.body()["previous_response_id"], "response_1")
        self.reply({"output_text": "fallback text"})
        self.assertEqual(self.client.respond("x").text, "fallback text")

    def test_anthropic_tools_and_reasoning_are_preserved(self):
        tool = {"type": "tool_use", "id": "a", "name": "weather", "input": {}}
        self.reply({"content": [{"type": "thinking", "thinking": "think"}, {"type": "text", "text": " answer\n"}, tool], "stop_reason": "tool_use"})
        answer = self.client.messages([{"role": "user", "content": "x"}], system="instructions", tools=[{"name": "weather", "input_schema": {"type": "object"}}])
        self.assertEqual(answer.text, " answer\n")
        self.assertEqual(answer.reasoning, "think")
        self.assertEqual(answer.tool_calls, [tool])
        self.assertTrue(answer.called_tool)
        self.assertEqual(self.body()["system"], "instructions")

    def test_transcription_keeps_full_json_available(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "sample.wav"
            file.write_bytes(b"RIFF-fixture")
            response = {"text": "OK", "segments": [{"start": 0.0, "text": "OK"}], "duration": 1.0}
            self.reply(response)
            self.assertEqual(self.client.transcription(file, response_format="verbose_json"), response)

    def test_response_metadata_is_available_for_cli_diagnostics(self):
        self.client.ask("x")
        self.assertEqual(self.client.last_status_code, 200)
        self.assertEqual(self.client.last_headers["x-router-request-id"], "sdk-test-id")

    def cli(self, *args):
        return subprocess.run([sys.executable, "test_router.py", "--env-file", "/nonexistent-sdk-fixture", "--router", self.url + "/v1", "--model", "public-chat", *args],
                              env=dict(os.environ, BRIGHTO_API_KEY="sk-brighto-test"), capture_output=True, text=True)

    def test_cli_uses_sdk_url_guardrails_and_reports_diagnostics(self):
        result = self.cli("--text", "Xin chào")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("status: 200", result.stdout)
        self.assertIn("sdk-test-id", result.stdout)
        self.assertEqual(self.server.calls[-1][1], "/v1/chat/completions")
        self.assertEqual(self.body()["messages"][0]["content"], "Xin chào")

    def test_cli_failure_and_dry_run_have_correct_exit_status(self):
        self.reply(b"circuit-open", status=503)
        result = self.cli()
        self.assertEqual(result.returncode, 1)
        self.assertIn("status: 503", result.stdout)
        self.assertIn("circuit-open", result.stdout)
        self.assertNotIn("Traceback", result.stderr)
        self.server.calls.clear()
        result = self.cli("--mode", "rerank", "--query", "speed", "--document", "fast", "--document", "slow", "--dry-run")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('"query": "speed"', result.stdout)
        self.assertEqual(self.server.calls, [])

    def test_cli_asr_and_stream_use_the_sdk(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "example.wav"
            file.write_bytes(b"RIFF-sdk-fixture")
            self.reply({"text": "recognized"})
            result = self.cli("--mode", "asr", "--file", str(file))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("recognized", result.stdout)
            self.assertEqual(self.server.calls[-1][1], "/v1/audio/transcriptions")
        self.reply(b'data: {"choices":[{"delta":{"content":"OK"}}]}\n\ndata: [DONE]\n\n', content_type="text/event-stream")
        result = self.cli("--stream")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("OK", result.stdout)
        self.assertTrue(self.body()["stream"])


if __name__ == "__main__":
    unittest.main()
