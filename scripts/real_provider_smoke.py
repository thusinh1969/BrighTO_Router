#!/usr/bin/env python3
"""Real-provider smoke: local llama.cpp (keyless) + DeepSeek V4 Pro, through the router.

Run: DEEPSEEK_KEY=sk-... python3 scripts/real_provider_smoke.py
Never prints the DeepSeek key. Sanitized output only.
"""
import os
import pathlib
import subprocess
import sys
import time

import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
BIN = REPO / "target/release/brighto-router"
PORT = 18088
BASE = "http://127.0.0.1:%d" % PORT


def load_env():
    env = {}
    for line in (REPO / ".env").read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#") and "=" in line:
            k, v = line.split("=", 1)
            env[k] = v
    return env


def main():
    env = load_env()
    admin_key = env["ADMIN_MASTER_KEY"]
    ds_key = os.environ["DEEPSEEK_KEY"]
    router = subprocess.Popen(
        [str(BIN)],
        env={**os.environ, "LISTEN_ADDR": "127.0.0.1:%d" % PORT,
             "DATA_DIR": "/tmp/brigto-data", "RUST_LOG": "error"},
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
    )
    try:
        for _ in range(120):
            try:
                if requests.get(BASE + "/healthz", timeout=1).status_code == 200:
                    break
            except Exception:
                pass
            time.sleep(0.5)
        admin = {"x-admin-key": admin_key, "Content-Type": "application/json"}
        checks = []

        def check(name, ok, detail=""):
            checks.append((name, ok))
            print(("PASS " if ok else "FAIL ") + name + (" " + str(detail) if detail else ""))

        # Local llama.cpp (keyless)
        r = requests.post(BASE + "/admin/backends", headers=admin, json={
            "name": "Local Qwen", "base_url": "http://127.0.0.1:8088/v1",
            "api_key_ref": "env:DUMMY_EMPTY", "format": "openai", "enabled": True}, timeout=60)
        check("create local backend", r.status_code == 200, r.status_code)
        lid = r.json()["id"]
        r = requests.get(BASE + "/admin/backends/%d/models" % lid, headers=admin, timeout=30)
        lm = r.json().get("models", [])
        check("local models", r.status_code == 200 and "qwen3.8-flash-next" in lm, lm)
        r = requests.post(BASE + "/admin/routes", headers=admin, json={
            "model_name": "qwen-local", "backend_ids": [lid],
            "provider_model_name": lm[0] if lm else "qwen3.8-flash-next",
            "first_byte_timeout": 300}, timeout=60)
        check("create local route", r.status_code == 200, r.status_code)
        r = requests.post(BASE + "/admin/keys", headers=admin, json={
            "team_id": 1, "owner": "real-test", "allowed_models": []}, timeout=60)
        ck = r.json()["key"]
        uh = {"Authorization": "Bearer " + ck, "Content-Type": "application/json"}
        r = requests.post(BASE + "/v1/chat/completions", headers=uh, json={
            "model": "qwen-local",
            "messages": [{"role": "user", "content": "Say hello in three words."}],
            "max_tokens": 16, "stream": False}, timeout=300)
        check("local llama.cpp chat", r.status_code == 200 and "choices" in r.json(), r.status_code)

        # DeepSeek V4 Pro
        r = requests.post(BASE + "/admin/backends", headers=admin, json={
            "name": "DeepSeek V4 Pro", "base_url": "https://api.deepseek.com",
            "api_key_ref": "env:DEEPSEEK_API_KEY", "format": "openai", "enabled": True}, timeout=60)
        did = r.json()["id"]
        r = requests.put(BASE + "/admin/backends/%d/key" % did, headers=admin, json={"key": ds_key}, timeout=60)
        check("set deepseek key (write-only)", r.status_code == 200 and r.json().get("key_resolved"), r.status_code)
        r = requests.get(BASE + "/admin/backends/%d/models" % did, headers=admin, timeout=30)
        dm = r.json().get("models", [])
        check("deepseek models", r.status_code == 200 and bool(dm), dm)
        r = requests.post(BASE + "/admin/routes", headers=admin, json={
            "model_name": "deepseek-v4-pro", "backend_ids": [did],
            "provider_model_name": "deepseek-v4-pro", "first_byte_timeout": 300}, timeout=60)
        r = requests.post(BASE + "/v1/chat/completions", headers=uh, json={
            "model": "deepseek-v4-pro",
            "messages": [{"role": "user", "content": "Say hello in three words."}],
            "max_tokens": 16, "stream": False}, timeout=300)
        check("deepseek chat", r.status_code == 200 and "choices" in r.json(), r.status_code)

        fails = [n for n, ok in checks if not ok]
        print("RESULT " + ("PASS" if not fails else "FAIL: " + ", ".join(fails)))
        sys.exit(0 if not fails else 1)
    finally:
        router.terminate()
        try:
            router.wait(timeout=5)
        except subprocess.TimeoutExpired:
            router.kill()


if __name__ == "__main__":
    main()
