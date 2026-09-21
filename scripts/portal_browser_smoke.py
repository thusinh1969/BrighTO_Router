#!/usr/bin/env python3
"""Browser smoke for the Admin Portal using Playwright.

Starts temporary Postgres, BrighTO-Router, and the Rust mock upstream. Then it
runs a real headless browser through the route wizard for embedding, rerank, and
ASR without spending live provider quota.
"""
from __future__ import annotations

import os
import pathlib
import socket
import subprocess
import sys
import tempfile
import textwrap
import time

import requests

REPO = pathlib.Path(__file__).resolve().parent.parent
ROUTER_BIN = REPO / "target/release/brighto-router"
MOCK_BIN = REPO / "target/release/brighto-router-mock"
ADMIN_KEY = "portal-browser-smoke-admin"
PLAYWRIGHT_RUNTIME = pathlib.Path("/tmp/brighto-playwright-runtime")
PLAYWRIGHT_VERSION = "1.63.0"


def free_port() -> int:
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def sh(*args: str, env: dict[str, str] | None = None, cwd: pathlib.Path = REPO) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=cwd, env=env, text=True, capture_output=True, check=True)


def ensure_binaries(env: dict[str, str]) -> None:
    missing = []
    if not ROUTER_BIN.exists():
        missing.append("brighto-router")
    if not MOCK_BIN.exists():
        missing.append("brighto-router-mock")
    if missing:
        print("Building release binaries for browser smoke: " + ", ".join(missing))
        sh("cargo", "build", "--release", "--locked", env=env)


def ensure_playwright(env: dict[str, str]) -> pathlib.Path:
    bin_path = PLAYWRIGHT_RUNTIME / "node_modules/.bin/playwright"
    if not bin_path.exists():
        PLAYWRIGHT_RUNTIME.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["npm", "install", "--prefix", str(PLAYWRIGHT_RUNTIME), "--silent", f"@playwright/test@{PLAYWRIGHT_VERSION}"],
            check=True,
            env=env,
            text=True,
        )
    return bin_path


def find_chrome_executable() -> str:
    candidates = []
    for key in ("CHROME_BIN", "GOOGLE_CHROME_SHIM"):
        value = os.environ.get(key)
        if value:
            candidates.append(pathlib.Path(value))
    candidates.extend(sorted(pathlib.Path.home().glob(".cache/selenium/chrome/linux64/*/chrome"), reverse=True))
    candidates.extend(sorted(pathlib.Path.home().glob(".cache/ms-playwright/chromium-*/chrome-linux64/chrome"), reverse=True))
    for path in candidates:
        if path.exists() and os.access(path, os.X_OK):
            return str(path)
    raise RuntimeError("Chrome/Chromium executable not found for Playwright smoke")


def wait_http(url: str, timeout_s: int = 60) -> None:
    deadline = time.time() + timeout_s
    last = None
    while time.time() < deadline:
        try:
            r = requests.get(url, timeout=1)
            if r.status_code < 500:
                return
            last = f"HTTP {r.status_code}"
        except Exception as exc:  # noqa: BLE001
            last = str(exc)
        time.sleep(0.5)
    raise RuntimeError(f"Timed out waiting for {url}: {last}")


def playwright_spec(base_url: str, admin_key: str, mock_url: str) -> str:
    return textwrap.dedent(
        f"""
        const {{ chromium, expect }} = require('@playwright/test');

        const baseURL = {base_url!r};
        const adminKey = {admin_key!r};
        const mockURL = {mock_url!r};

        function attachPageDiagnostics(page) {{
          page.on('pageerror', err => console.error('PAGEERROR ' + (err.stack || err.message))); 
          page.on('console', msg => {{ if (['error', 'warning'].includes(msg.type())) console.error('BROWSER ' + msg.type() + ' ' + msg.text()); }});
        }}

        async function login(page) {{
          await page.goto(baseURL + '/', {{ waitUntil: 'domcontentloaded' }});
          await expect(page.locator('#login-view')).toBeVisible();
          await page.fill('#login-user', 'admin');
          await page.fill('#login-pass', adminKey);
          await page.click('#login-submit');
          await expect(page.locator('#app-view')).toBeVisible();
          await expect(page.locator('#page-title')).toContainText('Dashboard');
        }}

        async function openAddModel(page) {{
          await page.click('#nav-models');
          await expect(page.locator('#page-title')).toContainText('Models');
          const addButton = page.getByRole('button', {{ name: /^Add model$/ }}).last();
          await expect(addButton).toBeVisible({{ timeout: 15000 }});
          await addButton.click();
          const modal = page.locator('#modal-overlay .modal').last();
          await expect(modal).toBeVisible();
          await expect(modal).toContainText('Task type');
          return modal;
        }}

        async function choosePickerModel(page, modelName) {{
          const picker = page.locator('.picker-overlay .modal').last();
          await expect(picker).toBeVisible();
          await picker.locator('.picker-search input').fill(modelName);
          await picker.getByRole('option').filter({{ hasText: modelName }}).first().click();
          await picker.getByRole('button', {{ name: 'Use this model' }}).click();
          await expect(picker).toHaveCount(0);
        }}

        async function createCustomRoute(page, task, providerModel, publicName) {{
          const modal = await openAddModel(page);
          const selects = modal.locator('select');
          const taskSelect = selects.nth(0);
          const providerSelect = selects.nth(1);
          await taskSelect.selectOption(task);
          await providerSelect.selectOption('custom-llm');
          const inputs = modal.locator('input');
          await inputs.nth(0).fill(mockURL);
          await modal.getByRole('button', {{ name: /Load models/i }}).click();
          await choosePickerModel(page, providerModel);
          await inputs.nth(3).fill(publicName);
          await modal.getByRole('button', {{ name: /^Test connection$/ }}).click();
          await expect(modal.locator('.connection-status')).toContainText('Connected', {{ timeout: 15000 }});
          await modal.getByRole('button', {{ name: /^Save enabled$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/, {{ timeout: 15000 }});
          await expect(page.locator('#content')).toContainText(publicName, {{ timeout: 15000 }});
        }}

        async function createModelGroup(page) {{
          const modal = await openAddModel(page);
          const selects = modal.locator('select');
          await selects.nth(0).selectOption('chat');
          await selects.nth(1).selectOption('custom-llm');
          await selects.nth(2).selectOption('group');
          await expect(modal).toContainText('Model Group endpoints');
          const inputs = modal.locator('input');
          await inputs.nth(0).fill(mockURL);
          await modal.getByRole('button', {{ name: /Load models/i }}).click();
          await choosePickerModel(page, 'mock-model');
          await inputs.nth(3).fill('browser-model-group');
          await modal.getByRole('button', {{ name: /^Test connection$/ }}).click();
          await expect(modal.locator('.connection-status')).toContainText('add this endpoint', {{ timeout: 15000 }});
          await modal.getByRole('button', {{ name: /^Add tested endpoint$/ }}).click();
          await expect(modal.locator('.group-endpoint-card')).toHaveCount(1);

          await inputs.nth(0).fill(mockURL + '/');
          await modal.getByRole('button', {{ name: /^Test connection$/ }}).click();
          await expect(modal.locator('.connection-status')).toContainText('add this endpoint', {{ timeout: 15000 }});
          await modal.getByRole('button', {{ name: /^Add tested endpoint$/ }}).click();
          await expect(modal.locator('.group-endpoint-card')).toHaveCount(2);
          await expect(modal.getByRole('button', {{ name: /^Save enabled$/ }})).toBeEnabled();
          await modal.getByRole('button', {{ name: /^Save enabled$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/, {{ timeout: 15000 }});
          await expect(page.locator('#content')).toContainText('browser-model-group', {{ timeout: 15000 }});
          const row = page.locator('.route-list-table tbody tr').filter({{ hasText: 'browser-model-group' }}).first();
          await expect(row).toContainText('Model Group');
          await expect(row).toContainText('2 endpoints');
        }}

        async function testedAdapterRoutes() {{
          const browser = await chromium.launch({{ executablePath: process.env.PLAYWRIGHT_CHROME_EXECUTABLE, headless: true, args: ['--no-sandbox'] }});
          const page = await browser.newPage({{ viewport: {{ width: 1440, height: 1000 }} }});
          attachPageDiagnostics(page);
          try {{
            await login(page);
            await createCustomRoute(page, 'embedding', 'mock-embedding', 'browser-embedding');
            await createCustomRoute(page, 'rerank', 'mock-rerank', 'browser-rerank');
            await createCustomRoute(page, 'asr', 'mock-asr', 'browser-asr');
            await createModelGroup(page);
            await page.reload({{ waitUntil: 'domcontentloaded' }});
            await expect(page.locator('#app-view')).toBeVisible();
            await expect(page.locator('#page-title')).toContainText(/Dashboard|Models/);
          }} finally {{
            await browser.close();
          }}
        }}

        async function providerTaskChoices() {{
          const browser = await chromium.launch({{ executablePath: process.env.PLAYWRIGHT_CHROME_EXECUTABLE, headless: true, args: ['--no-sandbox'] }});
          const page = await browser.newPage({{ viewport: {{ width: 1440, height: 1000 }} }});
          attachPageDiagnostics(page);
          try {{
            await login(page);
            const modal = await openAddModel(page);
            const selects = modal.locator('select');
            await selects.nth(0).selectOption('rerank');
            const providerSelect = selects.nth(1);
            await expect(providerSelect.locator('option[value="jina"]')).toHaveCount(1);
            await expect(providerSelect.locator('option[value="voyage"]')).toHaveCount(1);
            await expect(providerSelect.locator('option[value="cohere"]')).toHaveCount(1);
            await expect(providerSelect.locator('option[value="qwen"]')).toHaveCount(1);
            await providerSelect.selectOption('qwen');
            await expect(modal.locator('input').nth(0)).toHaveAttribute('placeholder', /workspace/);
            await expect(modal).toContainText('your workspace root');
            await expect(modal.locator('input').nth(2)).toHaveValue('qwen3-rerank');
            await providerSelect.selectOption('cohere');
            await expect(modal.locator('input').nth(2)).toHaveValue('rerank-v3.5');
            await expect(modal.locator('input').nth(3)).toHaveValue('rerank-v3.5');
            await modal.getByRole('button', {{ name: /Load models/i }}).click();
            const picker = page.locator('.picker-overlay .modal').last();
            await expect(picker).toContainText('rerank-v3.5');
            await expect(picker).not.toContainText('qwen3-rerank');
            await picker.getByRole('button', {{ name: 'Cancel' }}).click();
            await modal.getByRole('button', {{ name: 'Cancel' }}).click();
            await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/);
          }} finally {{
            await browser.close();
          }}
        }}

        (async () => {{
          await testedAdapterRoutes();
          console.log('PASS tested adapter route wizard');
          await providerTaskChoices();
          console.log('PASS provider task choices');
        }})().catch((err) => {{
          console.error(err && err.stack ? err.stack : err);
          process.exit(1);
        }});
        """
    )


def main() -> int:
    env = dict(os.environ)
    ensure_binaries(env)
    pg_name = f"brighto_portal_browser_{os.getpid()}"
    pg_port = free_port()
    router_port = free_port()
    mock_port = free_port()
    data_dir = tempfile.mkdtemp(prefix="brighto-portal-browser-data-")
    log_dir = pathlib.Path(tempfile.mkdtemp(prefix="brighto-portal-browser-logs-"))
    router = None
    mock = None
    try:
        sh(
            "docker", "run", "--rm", "-d", "--name", pg_name,
            "-e", "POSTGRES_DB=brighto_router",
            "-e", "POSTGRES_USER=brighto_router",
            "-e", "POSTGRES_PASSWORD=brighto_router_dev",
            "-p", f"127.0.0.1:{pg_port}:5432",
            "postgres:16-alpine",
        )
        for _ in range(60):
            r = subprocess.run(["docker", "exec", pg_name, "pg_isready", "-U", "brighto_router", "-d", "brighto_router"], capture_output=True)
            if r.returncode == 0:
                break
            time.sleep(1)
        db = f"postgres://brighto_router:brighto_router_dev@127.0.0.1:{pg_port}/brighto_router"
        migrate_env = dict(env, DATABASE_URL=db, PGPASSWORD="brighto_router_dev")
        if subprocess.run(["sqlx", "--version"], capture_output=True).returncode == 0:
            sh("sqlx", "migrate", "run", "--source", str(REPO / "migrations"), env=migrate_env)
        else:
            for migration in sorted((REPO / "migrations").glob("*.sql")):
                sh("psql", db, "-v", "ON_ERROR_STOP=1", "-f", str(migration), env=migrate_env)
        sh("psql", db, "-q", "-c", "INSERT INTO teams (name,budget,enabled) VALUES ('Browser Team',NULL,TRUE);", env=migrate_env)

        with (log_dir / "mock.log").open("w") as f:
            mock = subprocess.Popen(
                [str(MOCK_BIN)],
                cwd=REPO,
                env=dict(env, MOCK_ADDR=f"127.0.0.1:{mock_port}"),
                stdout=f,
                stderr=subprocess.STDOUT,
                text=True,
            )
        wait_http(f"http://127.0.0.1:{mock_port}/health", 30)

        with (log_dir / "router.log").open("w") as f:
            router = subprocess.Popen(
                [str(ROUTER_BIN)],
                cwd=REPO,
                env=dict(
                    env,
                    DATABASE_URL=db,
                    LISTEN_ADDR=f"127.0.0.1:{router_port}",
                    TLS_CERT_PATH="",
                    TLS_KEY_PATH="",
                    ADMIN_MASTER_KEY=ADMIN_KEY,
                    DATA_DIR=data_dir,
                    PORTAL_STATIC_FILE=str(REPO / "static/index.html"),
                    RUST_LOG="warn",
                ),
                stdout=f,
                stderr=subprocess.STDOUT,
                text=True,
            )
        base_url = f"http://127.0.0.1:{router_port}"
        wait_http(base_url + "/healthz", 60)

        spec_path = log_dir / "portal_browser.cjs"
        spec_path.write_text(playwright_spec(base_url, ADMIN_KEY, f"http://127.0.0.1:{mock_port}"))
        ensure_playwright(env)
        chrome_executable = find_chrome_executable()
        result = subprocess.run(
            ["node", str(spec_path)],
            cwd=REPO,
            env=dict(env, CI="1", NODE_PATH=str(PLAYWRIGHT_RUNTIME / "node_modules"), PLAYWRIGHT_CHROME_EXECUTABLE=chrome_executable),
            text=True,
            capture_output=True,
        )
        sys.stdout.write(result.stdout)
        sys.stderr.write(result.stderr)
        if result.returncode != 0:
            print(f"Logs kept in {log_dir}", file=sys.stderr)
            return result.returncode
        print("RESULT PASS")
        return 0
    finally:
        if router is not None:
            router.terminate()
            try:
                router.wait(timeout=5)
            except subprocess.TimeoutExpired:
                router.kill()
        if mock is not None:
            mock.terminate()
            try:
                mock.wait(timeout=5)
            except subprocess.TimeoutExpired:
                mock.kill()
        try:
            if 'spec_path' in locals() and spec_path.exists():
                spec_path.unlink()
        except OSError:
            pass
        subprocess.run(["docker", "rm", "-f", pg_name], capture_output=True)


if __name__ == "__main__":
    raise SystemExit(main())
