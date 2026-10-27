#!/usr/bin/env python3
"""Browser + live check for the single UAT Qwen 3.8 dual-protocol route."""

from __future__ import annotations

import json
import os
import pathlib
import re
import ssl
import sys
import urllib.error
import urllib.request

from playwright.sync_api import sync_playwright

UAT_DIR = pathlib.Path(__file__).resolve().parent
ENV_FILE = UAT_DIR / ".env.uat"
ARTIFACTS = UAT_DIR / "artifacts"
BASE = os.environ.get("UAT_BASE_URL", "https://127.0.0.1:18081").rstrip("/")
CA_CERT = os.environ.get("UAT_CA_CERT", "")
SSL_CONTEXT = (
    ssl.create_default_context(cafile=CA_CERT)
    if CA_CERT and os.path.exists(CA_CERT)
    else None
)


def load_env() -> None:
    if not ENV_FILE.exists():
        raise SystemExit(f"missing {ENV_FILE}; run uat/start.sh up")
    for line in ENV_FILE.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key, value)


def check(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)
    print("PASS " + message)


def request(path: str, body: dict | None = None) -> tuple[int, bytes]:
    data = None if body is None else json.dumps(body).encode()
    headers = {"x-api-key": os.environ.get("UAT_CLIENT_KEY", "")}
    if data is not None:
        headers["content-type"] = "application/json"
        headers["anthropic-version"] = "2023-06-01"
    req = urllib.request.Request(
        BASE + path,
        data=data,
        headers=headers,
        method="GET" if body is None else "POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=180, context=SSL_CONTEXT) as resp:
            return resp.status, resp.read()
    except urllib.error.HTTPError as error:
        return error.code, error.read()


def chrome_executable() -> str:
    candidates = []
    for key in ("PLAYWRIGHT_CHROME_EXECUTABLE", "CHROME_BIN", "GOOGLE_CHROME_SHIM"):
        value = os.environ.get(key)
        if value:
            candidates.append(pathlib.Path(value))
    candidates.extend(
        sorted(
            pathlib.Path.home().glob(".cache/ms-playwright/chromium-*/chrome-linux64/chrome"),
            reverse=True,
        )
    )
    for path in candidates:
        if path.exists() and os.access(path, os.X_OK):
            return str(path)
    raise RuntimeError("Chrome/Chromium executable not found")


def main() -> None:
    load_env()
    admin_key = os.environ.get("ADMIN_MASTER_KEY", "")
    client_key = os.environ.get("UAT_CLIENT_KEY", "")
    model = os.environ.get("UAT_QWEN_MODEL", "qwen3.8-flash-next")
    if not admin_key or not client_key:
        raise SystemExit("ADMIN_MASTER_KEY and UAT_CLIENT_KEY are required")
    ARTIFACTS.mkdir(parents=True, exist_ok=True)

    status, raw = request("/v1/models")
    check(status == 200, "UAT /v1/models returns 200")
    models = [item["id"] for item in json.loads(raw).get("data", [])]
    check(models == [model], f"UAT exposes exactly one model, got {models}")

    with sync_playwright() as p:
        browser = p.chromium.launch(
            executable_path=chrome_executable(),
            headless=True,
            args=["--no-sandbox"],
        )
        page = browser.new_page(
            viewport={"width": 1440, "height": 1000},
            ignore_https_errors=True,
        )
        page_errors: list[str] = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        try:
            page.goto(BASE + "/", wait_until="domcontentloaded")
            page.fill("#login-user", "admin")
            page.fill("#login-pass", admin_key)
            page.click("#login-submit")
            page.wait_for_selector("#app-view", state="visible", timeout=15000)
            page.locator('.nav[data-view="models"]').click()
            page.wait_for_selector("#page-title:has-text('Models')", timeout=15000)

            row = (
                page.locator(".route-list-table tbody tr")
                .filter(
                    has=page.locator(
                        "span.cell-main",
                        has_text=re.compile(rf"^{re.escape(model)}$"),
                    )
                )
                .first
            )
            row.wait_for(state="visible", timeout=15000)
            check(row.count() == 1, "Portal lists the single Qwen route")
            check("Claude CLI bridge" in row.inner_text(), "Qwen route shows the Claude CLI bridge badge")
            page.screenshot(path=str(ARTIFACTS / "uat-qwen-models.png"), full_page=True)

            row.get_by_role("button", name=re.compile("^Edit$")).click()
            modal = page.locator("#modal-overlay .modal").last
            modal.wait_for(state="visible")
            toggle = modal.locator("#route-bridge-toggle")
            check(toggle.is_checked(), "Qwen route bridge toggle is enabled")
            provider_model = (
                modal.locator(".route-model-grid .field").nth(0).locator("input").input_value()
            )
            check(provider_model == model, "Portal pre-fills the Qwen provider model")
            page.screenshot(path=str(ARTIFACTS / "uat-qwen-edit.png"), full_page=True)

            modal.get_by_role("button", name=re.compile("^Test connection$")).click()
            page.wait_for_selector(
                "#modal-overlay .connection-status:has-text('Connected')",
                timeout=180000,
            )
            check(True, "Portal Test connection reaches local Qwen")
            save_button = modal.get_by_role("button", name=re.compile("^Save enabled$"))
            check(not save_button.is_disabled(), "Save enabled is available after Qwen test")

            with page.expect_request(
                lambda request: "/admin/routes/" in request.url and request.method == "PATCH"
            ) as request_info:
                save_button.click()
            payload = request_info.value.post_data_json
            check(payload.get("model_name") == model, "Portal PATCH keeps the public model")
            check(payload.get("provider_model_name") == model, "Portal PATCH keeps the provider model")
            check(
                payload.get("protocol") == "local_openai_chat",
                "Portal PATCH uses local OpenAI chat protocol",
            )
            check(
                payload.get("accept_anthropic_messages") is True,
                "Portal PATCH sends accept_anthropic_messages=true",
            )
            page.locator("#modal-overlay").wait_for(state="hidden", timeout=15000)
            page.wait_for_selector(".route-list-table", state="visible")
            page.screenshot(path=str(ARTIFACTS / "uat-qwen-saved.png"), full_page=True)

            status, raw = request(
                "/v1/messages",
                {
                    "model": model,
                    "max_tokens": 128,
                    "messages": [
                        {"role": "user", "content": "Reply with exactly QWEN_UI_ANTHROPIC_OK."}
                    ],
                },
            )
            message = json.loads(raw) if raw else {}
            check(status == 200, "Qwen /v1/messages after Portal save returns 200")
            check(message.get("type") == "message", "Qwen /v1/messages returns an Anthropic message")
            text = "".join(
                block.get("text", "")
                for block in message.get("content", [])
                if block.get("type") == "text"
            )
            check(bool(text.strip()), "Qwen /v1/messages returns text")

            status, raw = request(
                "/v1/chat/completions",
                {
                    "model": model,
                    "max_tokens": 64,
                    "messages": [{"role": "user", "content": "Reply with exactly QWEN_UI_CHAT_OK."}],
                },
            )
            chat = json.loads(raw) if raw else {}
            check(status == 200, "Qwen /v1/chat/completions after Portal save returns 200")
            check(bool(chat.get("choices")), "Qwen /v1/chat/completions returns choices")
            check(not page_errors, "no browser page errors")
        finally:
            browser.close()


if __name__ == "__main__":
    try:
        main()
    except AssertionError as error:
        print("FAIL " + str(error), file=sys.stderr)
        sys.exit(1)
