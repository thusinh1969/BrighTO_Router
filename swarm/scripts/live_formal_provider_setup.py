#!/usr/bin/env python3
"""Clean live BrighTO provider data and create one smoke route per usable provider.

Reads .env for admin URL/key and .model for provider API keys. Never prints secrets.
"""
from __future__ import annotations

import json
import os
import re
import ssl
import subprocess
import sys
import time
from pathlib import Path
from urllib import error, parse, request

ROOT = Path(__file__).resolve().parents[2]
ENV = ROOT / ".env"
MODEL = ROOT / ".model"
BASE = os.environ.get("BRIGHTO_BASE_URL") or "https://127.0.0.1:18443"
ADMIN = os.environ.get("BRIGHTO_ADMIN_KEY")


def read_env() -> dict[str, str]:
    out = {}
    if ENV.exists():
        for raw in ENV.read_text(errors="ignore").splitlines():
            line = raw.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            k, v = line.split("=", 1)
            out[k.strip()] = v.strip().strip('"').strip("'")
    return out


def ensure_data_dir_env() -> None:
    if not ENV.exists():
        return
    text = ENV.read_text(errors="ignore")
    if "DATA_DIR=" in text:
        new = "\n".join("DATA_DIR=/var/lib/brighto-router" if x.startswith("DATA_DIR=") else x for x in text.splitlines()) + "\n"
    else:
        new = text.rstrip() + "\nDATA_DIR=/var/lib/brighto-router\n"
    if new != text:
        ENV.write_text(new)
        print("env: DATA_DIR=/var/lib/brighto-router set (local .env, not printed secrets)")


def parse_model_file() -> list[dict[str, str | None]]:
    if not MODEL.exists():
        return []
    text = MODEL.read_text(errors="ignore")
    items = []
    for m in re.finditer(r"\{[^{}]*\}", text, re.S):
        block = m.group(0)

        def val(name: str) -> str | None:
            mm = re.search(r'"' + re.escape(name) + r'"\s*:\s*(null|"(?:[^"\\]|\\.)*")', block)
            if not mm:
                return None
            raw = mm.group(1)
            if raw == "null":
                return None
            return bytes(raw[1:-1], "utf-8").decode("unicode_escape")

        item = {k: val(k) for k in ("model_name", "provider", "api_base", "api_key", "model_id")}
        if item.get("model_name") or item.get("provider"):
            items.append(item)
    return items


def pick(items: list[dict[str, str | None]], pred, preferred: list[str] = ()) -> dict[str, str | None] | None:
    matches = [x for x in items if pred(x) and x.get("api_key")]
    for name in preferred:
        for x in matches:
            if x.get("model_name") == name:
                return x
    return matches[0] if matches else None


def http(path: str, method: str = "GET", body=None, admin: bool = True, bearer: str | None = None, timeout: int = 60):
    data = None if body is None else json.dumps(body).encode()
    headers = {"content-type": "application/json"}
    if admin:
        headers["x-admin-key"] = ADMIN or ""
    if bearer:
        headers["authorization"] = f"Bearer {bearer}"
    req = request.Request(BASE + path, data=data, method=method, headers=headers)
    ctx = ssl._create_unverified_context()
    try:
        with request.urlopen(req, context=ctx, timeout=timeout) as resp:
            txt = resp.read().decode(errors="replace")
            return resp.status, json.loads(txt) if txt else None, txt
    except error.HTTPError as e:
        txt = e.read().decode(errors="replace")
        try:
            parsed = json.loads(txt)
        except Exception:
            parsed = None
        return e.code, parsed, txt


def must(path: str, method: str = "GET", body=None):
    code, parsed, txt = http(path, method, body)
    if code < 200 or code >= 300:
        raise RuntimeError(f"{method} {path} -> {code} {txt[:240]}")
    return parsed


def restart_router_if_needed() -> None:
    # The caller normally recreated already. This function is intentionally no-op unless explicitly requested.
    if os.environ.get("BRIGHTO_RECREATE_ROUTER") != "1":
        return
    subprocess.run(["docker", "compose", "up", "-d", "--force-recreate", "router"], cwd=ROOT, check=True)
    for _ in range(60):
        out = subprocess.run(["docker", "inspect", "--format", "{{.State.Health.Status}}", "brighto-airouter-router-1"], text=True, capture_output=True)
        if out.stdout.strip() == "healthy":
            return
        time.sleep(1)
    raise RuntimeError("router did not become healthy")


def main() -> int:
    global ADMIN
    env = read_env()
    ADMIN = ADMIN or env.get("ADMIN_MASTER_KEY") or env.get("BRIGHTO_ADMIN_KEY")
    if not ADMIN:
        print("missing admin key", file=sys.stderr)
        return 2
    ensure_data_dir_env()
    restart_router_if_needed()

    items = parse_model_file()
    selected = {
        "openai": pick(items, lambda x: x.get("provider") == "openai", ["gpt-5.5", "gpt-5.6"]),
        "anthropic": pick(items, lambda x: x.get("provider") == "anthropic"),
        "deepseek": pick(items, lambda x: x.get("provider") == "deepseek", ["deepseek-v4-pro", "deepseek-v4-flash"]),
        "kimi": pick(items, lambda x: x.get("provider") in ("moonshot", "kimi"), ["kimi-k3"]),
        "qwen": pick(items, lambda x: x.get("provider") == "qwen" and "token-plan" not in (x.get("api_base") or ""), ["qwen-3.8-max"]),
        "zai": pick(items, lambda x: (x.get("model_name") or "").lower().startswith("glm") or "z.ai" in (x.get("api_base") or ""), ["glm-5.2"]),
    }
    # Use cheap/stable public provider model names for smoke. The API key still comes from .model.
    provider_model_override = {
        "openai": "gpt-4o-mini",
        "qwen": "qwen3.8-max",
    }

    routes_to_delete = must("/admin/routes")
    for r in routes_to_delete:
        code, _, txt = http("/admin/routes/" + parse.quote(r["model_name"], safe=""), "DELETE")
        print(f"route delete {r['model_name']}: {code}")

    # Patch canonical provider rows. Keep exactly these ten providers.
    provider_specs = {
        1: {"name": "openai", "base_url": "https://api.openai.com", "format": "openai", "enabled": bool(selected["openai"])},
        2: {"name": "anthropic", "base_url": "https://api.anthropic.com", "format": "anthropic", "enabled": bool(selected["anthropic"])},
        3: {"name": "gemini", "base_url": "https://generativelanguage.googleapis.com/v1beta/openai", "format": "openai", "enabled": False},
        4: {"name": "deepseek", "base_url": "https://api.deepseek.com", "format": "openai", "enabled": bool(selected["deepseek"])},
        5: {"name": "kimi", "base_url": (selected["kimi"] or {}).get("api_base") or "https://api.moonshot.ai/v1", "format": "openai", "enabled": bool(selected["kimi"])},
        6: {"name": "qwen", "base_url": (selected["qwen"] or {}).get("api_base") or "https://dashscope-intl.aliyuncs.com/compatible-mode/v1", "format": "openai", "enabled": bool(selected["qwen"])},
        7: {"name": "zai", "base_url": (selected["zai"] or {}).get("api_base") or "https://api.z.ai/api/paas/v4", "format": "openai", "enabled": bool(selected["zai"])},
        8: {"name": "openrouter", "base_url": "https://openrouter.ai/api/v1", "format": "openai", "enabled": bool(env.get("OPENROUTER_API_KEY"))},
        9: {"name": "meta-muse", "base_url": "https://api.meta.ai/v1", "format": "openai", "enabled": bool(env.get("META_MUSE_API_KEY"))},
        10: {"name": "custom-local-llama", "base_url": "http://127.0.0.1:8088/v1", "format": "openai", "enabled": True},
    }
    current = {b["id"]: b for b in must("/admin/backends")}
    for bid, spec in provider_specs.items():
        if bid in current:
            payload = {**spec, "weight": 1, "max_inflight": 0}
            must(f"/admin/backends/{bid}", "PATCH", payload)
            print(f"provider keep/patch id={bid} name={spec['name']} enabled={spec['enabled']}")
        else:
            print(f"provider id={bid} missing; cannot preserve canonical id via API")

    # Delete every non-canonical provider after routes are gone.
    for b in must("/admin/backends"):
        if b["id"] not in provider_specs:
            code, _, txt = http(f"/admin/backends/{b['id']}", "DELETE")
            print(f"provider delete id={b['id']} name={b['name']}: {code}")

    route_specs = []
    def add_cloud(public: str, provider_id: int, item: dict[str, str | None] | None, protocol: str, auth: str):
        if not item or not item.get("api_key"):
            print(f"route skip {public}: missing key/model")
            return
        provider_key = "openai" if provider_id == 1 else "qwen" if provider_id == 6 else ""
        route_specs.append({
            "public": public,
            "provider_id": provider_id,
            "provider_model": provider_model_override.get(provider_key) or item["model_name"],
            "protocol": protocol,
            "auth_mode": auth,
            "provider_key": item["api_key"],
        })
    add_cloud("test-openai", 1, selected["openai"], "openai_chat", "bearer")
    add_cloud("test-anthropic", 2, selected["anthropic"], "anthropic_messages", "anthropic")
    add_cloud("test-deepseek", 4, selected["deepseek"], "openai_chat", "bearer")
    add_cloud("test-kimi", 5, selected["kimi"], "openai_chat", "bearer")
    add_cloud("test-qwen", 6, selected["qwen"], "openai_chat", "bearer")
    add_cloud("test-zai", 7, selected["zai"], "openai_chat", "bearer")
    route_specs.append({"public": "test-custom-local", "provider_id": 10, "provider_model": "qwen3.8-flash-next", "protocol": "local_openai_chat", "auth_mode": "none"})

    for spec in route_specs:
        payload = {
            "model_name": spec["public"],
            "backend_ids": [spec["provider_id"]],
            "provider_model_name": spec["provider_model"],
            "protocol": spec["protocol"],
            "auth_mode": spec["auth_mode"],
            "chars_per_token": 4.0,
            "first_byte_timeout": 180,
            "max_output_tokens": 64,
            "enabled": True,
        }
        if spec.get("provider_key"):
            payload["provider_key"] = spec["provider_key"]
        res = must("/admin/routes", "POST", payload)
        print(f"route upsert {spec['public']} -> provider_model={spec['provider_model']} protocol={spec['protocol']} auth={spec['auth_mode']}")

    # Create one revealable client key allowed for all test routes.
    teams = must("/admin/teams")
    default_team = next((t for t in teams if t.get("name") == "Default Team"), teams[0] if teams else None)
    if not default_team:
        default_team = must("/admin/teams", "POST", {"name": "Default Team", "enabled": True, "budget": None})
    route_names = [r["public"] for r in route_specs]
    key_resp = must("/admin/keys", "POST", {"team_id": default_team["id"], "owner": "formal-provider-smoke", "allowed_models": route_names, "rpm_limit": 20, "concurrency_limit": 2, "budget": None})
    client_key = key_resp.get("key")
    print(f"client key created prefix={key_resp.get('prefix')} allowed={len(route_names)} routes")

    time.sleep(35)  # allow config poll + health loop; cloud routes must not be poisoned after this.
    results = []
    for spec in route_specs:
        if spec["protocol"] == "anthropic_messages":
            path = "/v1/messages"
            body = {"model": spec["public"], "messages": [{"role": "user", "content": "Reply exactly OK."}], "max_tokens": 8}
        else:
            path = "/v1/chat/completions"
            body = {"model": spec["public"], "messages": [{"role": "user", "content": "Reply exactly OK."}], "max_tokens": 8, "stream": False}
            if spec["public"] == "test-openai":
                body.pop("max_tokens", None)
                body["max_completion_tokens"] = 8
        code, parsed, txt = http(path, "POST", body, admin=False, bearer=client_key, timeout=90)
        ok = 200 <= code < 300
        results.append((spec["public"], code, ok, txt[:220]))
        print(f"smoke {spec['public']}: path={path} status={code} ok={ok}")
        if code == 429:
            print(f"smoke {spec['public']}: rate limited, no retry")
    print("summary:")
    for name, code, ok, txt in results:
        if ok:
            print(f"  PASS {name} status={code}")
        else:
            # sanitize any long token-like strings defensively
            safe = re.sub(r"sk-[A-Za-z0-9._-]+", "sk-<redacted>", txt)
            safe = re.sub(r"[A-Za-z0-9_\-]{40,}", "<redacted-long-token>", safe)
            print(f"  FAIL {name} status={code} body={safe}")
    return 0 if all(ok for _, _, ok, _ in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
