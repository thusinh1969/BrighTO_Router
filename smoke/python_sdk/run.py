#!/usr/bin/env python3
"""Verify brighto.py against the Docker runtime over HTTP and verified HTTPS.

Runs isolated PostgreSQL databases, the local Docker router image, Rust mocks,
and the existing Playwright route wizard audit. Never changes the installed
router, .env, or production database. No paid provider calls.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import tempfile
import time

import requests

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO))
sys.path.insert(0, str(REPO / "scripts"))
from brighto import Router, RouterError
import portal_browser_smoke as browser_smoke


def command(*args, input=None, env=None):
    return subprocess.run(args, input=input, env=env, cwd=REPO, check=True, capture_output=True, text=True).stdout.strip()


def wait_http(url, verify=True):
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        try:
            if requests.get(url, verify=verify, timeout=2).status_code == 200:
                return
        except requests.RequestException:
            pass
        time.sleep(0.25)
    raise RuntimeError("Service did not become ready: " + url)


def run_matrix(base, key, verify):
    with Router(base, key, "browser-chat-a", verify=verify, timeout=30) as client:
        names = client.models()
        assert "browser-model-group" in names and "sdk-responses" in names
        answer = client.ask("Reply OK", max_tokens=16)
        assert answer.content and answer.usage
        assert client.chat([{"role": "user", "content": "first"}, {"role": "assistant", "content": "reply"}, {"role": "user", "content": "next"}], system="Be brief").text
        assert "".join(client.stream("Reply OK", max_tokens=16))
        events = list(client.events("/chat/completions", messages=[{"role": "user", "content": "OK"}], stream_options={"include_usage": True}))
        assert any("usage" in event for event in events)
        assert client.complete("Reply OK", model="browser-completion").text
        assert client.respond("Reply OK", model="sdk-responses").text
        assert client.messages([{"role": "user", "content": "OK"}], model="sdk-anthropic").text
        assert any(event.get("type") == "message_stop" for event in client.events("/messages", model="sdk-anthropic", max_tokens=16, messages=[{"role": "user", "content": "OK"}]))
        embedding = client.embed("hello", model="browser-embedding")
        assert isinstance(embedding, list) and embedding
        ranked = client.rerank("fast router", ["fast Rust router", "unrelated text", "AI gateway"], model="browser-rerank", top_n=2)
        assert ranked.get("results") or ranked.get("data")
        assert client.transcribe(REPO / "tests/fixtures/asr_smoke.wav", model="browser-asr")
        questions = {"ok": {"type": "noul", "instructions": "Is the message OK?"}}
        assert client.systemone({"message": "OK"}, questions, model="browser-systemone-a")["answers"]
        assert client.decisions({"message": "OK"}, questions, model="browser-systemone-group")["answers"]
        assert client.post("/v1/decisions", model="browser-systemone-group", state={"message": "OK"}, questions=questions)["answers"]
        for group in ("browser-model-group", "browser-weighted-group"):
            for _ in range(4):
                assert client.ask("OK", model=group).text
        for root in (base, base + "/v1", base + "/v1/v1/"):
            with Router(root, key, "browser-chat-a", verify=verify) as alternate:
                assert alternate.ask("OK").text
        try:
            client.embed("wrong type", model="browser-chat-a")
            raise AssertionError("Wrong-type route was accepted")
        except RouterError as error:
            assert error.status_code == 400 and error.request_id
        try:
            client.ask("OK", model="missing-model")
            raise AssertionError("Missing model was accepted")
        except RouterError as error:
            assert error.status_code == 404
    with Router(base, "sk-invalid", "browser-chat-a", verify=verify) as invalid:
        try:
            invalid.ask("OK")
            raise AssertionError("Invalid client key was accepted")
        except RouterError as error:
            assert error.status_code == 401
    print("PASS SDK API matrix: chat, history, streaming, events, completions, Responses, Anthropic, embeddings, rerank, ASR, decisions, round-robin/weighted groups, URL guardrails, errors", flush=True)
    cli_cases = [
        ("chat", "browser-chat-a", []),
        ("chat", "browser-model-group", ["--stream"]),
        ("completions", "browser-completion", []),
        ("responses", "sdk-responses", []),
        ("messages", "sdk-anthropic", []),
        ("embeddings", "browser-embedding", []),
        ("rerank", "browser-rerank", ["--query", "fast router", "--document", "fast Rust router", "--document", "unrelated document", "--top-n", "1"]),
        ("asr", "browser-asr", ["--file", str(REPO / "tests/fixtures/asr_smoke.wav")]),
        ("systemone", "browser-systemone-group", []),
    ]
    for mode, model, extra in cli_cases:
        args = [sys.executable, "test_router.py", "--router", base + "/v1", "--env-file", "/nonexistent-sdk-fixture", "--mode", mode, "--model", model, *extra]
        if isinstance(verify, str):
            args += ["--ca", verify]
        output = command(*args, env=dict(os.environ, BRIGHTO_API_KEY=key))
        assert "status: 200" in output and "x-router-request-id:" in output, output
    print("PASS test_router.py: all eight modes + streamed Model Group, via SDK", flush=True)


def configure_route(base, headers, model, upstream, protocol, dialect="openai", provider_model="mock-model", verify=True):
    def admin(path, body):
        response = requests.post(base + "/admin/" + path, headers=headers, json=body, verify=verify, timeout=60)
        response.raise_for_status()
        return response.json()
    backend = admin("backends", {"name": model, "base_url": upstream, "api_key_ref": "env:NONE", "format": dialect, "enabled": True})
    tested = admin("test-connection", {"base_url": upstream, "dialect": dialect, "auth_mode": "none", "protocol": protocol, "provider_model_name": provider_model})
    assert tested.get("ok"), tested
    admin("routes", {"model_name": model, "backend_ids": [backend["id"]], "provider_model_name": provider_model, "protocol": protocol, "auth_mode": "none", "enabled": True})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="thusinh1969/brighto_airouter:v1.1.0", help="Existing local router Docker image; never pulled or pushed")
    parser.add_argument("--local-llm", help="Optional no-auth OpenAI-compatible backend URL, including /v1")
    args = parser.parse_args()
    env = dict(os.environ)
    browser_smoke.ensure_binaries(env)
    browser_smoke.ensure_playwright(env)
    image = command("docker", "image", "inspect", args.image, "--format", "{{.Id}}")
    print("Testing Docker image " + image, flush=True)
    hostname = socket.gethostname()
    pg = "brighto-sdk-pg-" + secrets.token_hex(4)
    pg_port, mock_port = browser_smoke.free_port(), browser_smoke.free_port()
    admin_key = "sdk-admin-" + secrets.token_hex(16)
    router_names = []
    mock = None
    with tempfile.TemporaryDirectory(prefix="brighto-sdk-smoke-") as directory:
        temp = Path(directory)
        temp.chmod(0o755)
        cert, private = temp / "cert.pem", temp / "key.pem"
        command("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-keyout", str(private), "-out", str(cert), "-subj", "/CN=localhost", "-addext", f"subjectAltName=DNS:localhost,DNS:{hostname},IP:127.0.0.1")
        private.chmod(0o644)  # Ephemeral test certificate, readable by non-root Docker user.
        try:
            command("docker", "run", "--rm", "-d", "--name", pg, "-e", "POSTGRES_USER=sdk", "-e", "POSTGRES_PASSWORD=sdk-test-only", "-p", f"127.0.0.1:{pg_port}:5432", "postgres:16-alpine")
            for _ in range(60):
                if subprocess.run(["docker", "exec", pg, "pg_isready", "-U", "sdk"], capture_output=True).returncode == 0:
                    break
                time.sleep(0.5)
            mock_log = (temp / "mock.log").open("w")
            mock = subprocess.Popen([str(browser_smoke.MOCK_BIN)], cwd=REPO, env=dict(env, MOCK_ADDR=f"0.0.0.0:{mock_port}"), stdout=mock_log, stderr=subprocess.STDOUT)
            mock_base = f"http://{hostname}:{mock_port}"
            wait_http(mock_base + "/health")
            for scheme in ("http", "https"):
                db_name = "sdk_" + scheme
                command("docker", "exec", pg, "psql", "-U", "sdk", "-c", f"CREATE DATABASE {db_name}")
                for migration in sorted((REPO / "migrations").glob("*.sql")):
                    command("docker", "exec", "-i", pg, "psql", "-U", "sdk", "-d", db_name, "-v", "ON_ERROR_STOP=1", input=migration.read_text())
                command("docker", "exec", pg, "psql", "-U", "sdk", "-d", db_name, "-c", "INSERT INTO teams(name,budget,enabled) VALUES ('SDK Test Team',NULL,TRUE)")
                port = browser_smoke.free_port()
                base = f"{scheme}://{hostname}:{port}"
                verify = str(cert) if scheme == "https" else True
                name = "brighto-sdk-" + scheme + "-" + secrets.token_hex(4)
                router_names.append(name)
                docker_args = ["docker", "run", "--rm", "-d", "--name", name, "--network", "host", "-e", f"DATABASE_URL=postgres://sdk:sdk-test-only@127.0.0.1:{pg_port}/{db_name}", "-e", f"LISTEN_ADDR=0.0.0.0:{port}", "-e", f"ADMIN_MASTER_KEY={admin_key}", "-e", "ADMIN_ALLOW_CIDR=0.0.0.0/0,::/0", "-e", "RUST_LOG=warn"]
                if scheme == "https":
                    docker_args += ["-v", f"{temp}:/certs:ro", "-e", "TLS_CERT_PATH=/certs/cert.pem", "-e", "TLS_KEY_PATH=/certs/key.pem"]
                command(*docker_args, image)
                wait_http(base + "/readyz", verify)
                if scheme == "https":
                    try:
                        with Router(base, "sk-test", verify=True) as untrusted:
                            untrusted.models()
                        raise AssertionError("Self-signed certificate should not be trusted by default")
                    except RouterError as error:
                        assert "SSLError" in str(error)
                    print("PASS HTTPS rejects untrusted certificate; verified certificate is used for SDK calls", flush=True)
                # Real embedded Portal: create/test/save no-auth routes and both group strategies.
                spec = browser_smoke.playwright_spec(base, admin_key, mock_base + "/v1")
                spec = spec.replace("viewport:", "ignoreHTTPSErrors: true, viewport:")
                spec_path = temp / ("portal-" + scheme + ".cjs")
                spec_path.write_text(spec)
                output = command("node", str(spec_path), env=dict(env, NODE_PATH=str(browser_smoke.PLAYWRIGHT_RUNTIME / "node_modules"), PLAYWRIGHT_CHROME_EXECUTABLE=browser_smoke.find_chrome_executable()))
                print(scheme.upper() + " Playwright: " + output, flush=True)
                headers = {"x-admin-key": admin_key}
                configure_route(base, headers, "sdk-responses", mock_base + "/v1", "openai_responses", provider_model="mock-responses", verify=verify)
                configure_route(base, headers, "sdk-anthropic", mock_base + "/v1", "anthropic_messages", dialect="anthropic", verify=verify)
                response = requests.post(base + "/admin/keys", headers=headers, json={"team_id": 1, "owner": "sdk-smoke", "allowed_models": []}, verify=verify, timeout=30)
                response.raise_for_status()
                key = response.json()["key"]
                run_matrix(base, key, verify)
                if args.local_llm:
                    available = requests.get(args.local_llm.rstrip("/") + "/models", timeout=5)
                    available.raise_for_status()
                    provider_model = available.json()["data"][0]["id"]
                    configure_route(base, headers, "sdk-live-local", args.local_llm, "local_openai_chat", provider_model=provider_model, verify=verify)
                    with Router(base, key, "sdk-live-local", verify=verify, timeout=90) as client:
                        answer = client.ask("Reply OK in one short sentence.", max_tokens=32, thinking=False)
                        assert answer.content, "Local model returned no text"
                        assert "".join(client.stream("Reply OK.", max_tokens=32, thinking=False))
                    print("PASS " + scheme.upper() + " real local LLM: chat and streaming", flush=True)
                time.sleep(6)  # Beyond the router's 5-second backend health interval.
                with Router(base, key, "browser-model-group", verify=verify) as client:
                    assert client.ask("OK after health check").text
                    if args.local_llm:
                        assert client.ask("Reply OK.", model="sdk-live-local", max_tokens=32, thinking=False).text
                print("PASS " + scheme.upper() + " gateway calls after backend health interval", flush=True)
                command("docker", "rm", "-f", "-v", name)
                router_names.remove(name)
            print("RESULT PASS: HTTP + verified HTTPS SDK and Docker Portal flows", flush=True)
        except subprocess.CalledProcessError as error:
            print(error.stderr, file=sys.stderr)
            raise
        finally:
            for name in router_names:
                logs = subprocess.run(["docker", "logs", "--tail", "30", name], capture_output=True, text=True)
                print(logs.stdout + logs.stderr, file=sys.stderr)
                subprocess.run(["docker", "rm", "-f", "-v", name], capture_output=True)
            if mock is not None:
                mock.terminate()
                try:
                    mock.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    mock.kill()
                    mock.wait()
                mock_log.close()
            subprocess.run(["docker", "rm", "-f", "-v", pg], capture_output=True)


if __name__ == "__main__":
    main()
