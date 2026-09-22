#!/usr/bin/env python3
"""Full Playwright audit for the BrighTO Admin Portal.

Runs an isolated local stack: temporary PostgreSQL, local brighto-router binary,
Rust mock upstream, and headless Chromium. The audit clicks through every main
Portal section and exercises create/edit/delete flows that have broken before.
"""
from __future__ import annotations

import os
import sys
import textwrap

import portal_browser_smoke as smoke


def full_playwright_spec(base_url: str, admin_key: str, mock_url: str) -> str:
    return textwrap.dedent(
        f"""
        const {{ chromium, expect }} = require('@playwright/test');

        const baseURL = {base_url!r};
        const adminKey = {admin_key!r};
        const mockURL = {mock_url!r};

        function attachPageDiagnostics(page) {{
          page.on('pageerror', err => console.error('PAGEERROR ' + (err.stack || err.message)));
          page.on('console', msg => {{
            if (['error', 'warning'].includes(msg.type())) console.error('BROWSER ' + msg.type() + ' ' + msg.text());
          }});
          page.on('response', resp => {{
            const url = resp.url();
            if (url.includes('/admin/') && resp.status() >= 500) console.error('ADMIN_5XX ' + resp.status() + ' ' + url);
          }});
        }}

        async function adminFetch(path, method = 'GET', body = undefined) {{
          const resp = await fetch(baseURL + path, {{
            method,
            headers: {{ 'content-type': 'application/json', 'x-admin-key': adminKey }},
            body: body === undefined ? undefined : JSON.stringify(body),
          }});
          const text = await resp.text();
          let data = null;
          try {{ data = text ? JSON.parse(text) : null; }} catch {{ data = text; }}
          if (!resp.ok) throw new Error(method + ' ' + path + ' -> ' + resp.status + ' ' + text);
          return data;
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

        async function nav(page, view, title) {{
          await page.locator('.nav[data-view="' + view + '"]').click();
          await expect(page.locator('#page-title')).toContainText(title, {{ timeout: 15000 }});
          await expect(page.locator('#content')).toBeVisible();
          await expect(page.locator('#toast.show')).toHaveCount(0);
        }}

        async function auditNavigation(page) {{
          await nav(page, 'dashboard', 'Dashboard');
          await nav(page, 'providers', 'Providers');
          await nav(page, 'models', 'Models');
          await nav(page, 'teams', 'Teams');
          await nav(page, 'keys', 'API Keys');
          await nav(page, 'usage', 'Usage');
          await nav(page, 'settings', 'Settings');
        }}

        async function openAddModel(page) {{
          await nav(page, 'models', 'Models');
          const addButton = page.getByRole('button', {{ name: /^Add model route$/ }}).last();
          await expect(addButton).toBeVisible({{ timeout: 15000 }});
          await addButton.click();
          const modal = page.locator('#modal-overlay .modal').last();
          await expect(modal).toBeVisible();
          await expect(modal).toContainText('Task type');
          return modal;
        }}

        async function openCreateModelGroup(page) {{
          await nav(page, 'models', 'Models');
          const groupButton = page.getByRole('button', {{ name: /^Create model group$/ }}).last();
          await expect(groupButton).toBeVisible({{ timeout: 15000 }});
          await groupButton.click();
          const modal = page.locator('#modal-overlay .modal').last();
          await expect(modal).toBeVisible();
          await expect(modal).toContainText('Create model group');
          await expect(modal).toContainText('Model Group');
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

        async function createCustomRoute(page, task, providerModel, publicName, baseUrl = mockURL) {{
          const modal = await openAddModel(page);
          const selects = modal.locator('select');
          await selects.nth(0).selectOption(task);
          await selects.nth(1).selectOption('custom-llm');
          const inputs = modal.locator('input');
          await inputs.nth(0).fill(baseUrl);
          await modal.getByRole('button', {{ name: /Load models/i }}).click();
          await choosePickerModel(page, providerModel);
          await inputs.nth(3).fill(publicName);
          await modal.getByRole('button', {{ name: /^Test connection$/ }}).click();
          await expect(modal.locator('.connection-status')).toContainText('Connected', {{ timeout: 15000 }});
          await modal.getByRole('button', {{ name: /^Save enabled$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/, {{ timeout: 15000 }});
          await expect(page.locator('#content')).toContainText(publicName, {{ timeout: 15000 }});
        }}

        async function createModelGroupRoute(page) {{
          let payload = null;
          await page.route('**/admin/routes', async route => {{
            if (route.request().method() === 'POST') {{
              const body = route.request().postDataJSON();
              if (body.model_name === 'audit-model-group') {{
                payload = body;
                expect(body.protocol).toBe('openai_chat');
                expect(body.routing_policy).toBe('weighted_round_robin');
                expect(body.endpoints).toHaveLength(2);
                expect(body.endpoints[0].weight).toBe(2);
                expect(body.endpoints[1].weight).toBe(1);
                for (const ep of body.endpoints) expect(ep.provider_key).toBeUndefined();
              }}
            }}
            await route.continue();
          }});
          const modal = await openCreateModelGroup(page);
          await expect(modal).toContainText('Add existing tested route');
          await expect(modal).toContainText('No provider key is entered here');
          await expect(modal).not.toContainText('Provider API key');
          const selects = modal.locator('select');
          await selects.nth(0).selectOption('chat');
          await selects.nth(1).selectOption('weighted_round_robin');
          const inputs = modal.locator('input');
          await inputs.nth(0).fill('audit-model-group');
          await selects.nth(2).selectOption('audit-chat');
          await modal.getByRole('button', {{ name: /^Add route to group$/ }}).click();
          await expect(modal.locator('.route-picker-row')).toHaveCount(1);
          await expect(modal.getByRole('button', {{ name: /^Save enabled$/ }})).toBeDisabled();
          await selects.nth(2).selectOption('audit-chat-b');
          await modal.getByRole('button', {{ name: /^Add route to group$/ }}).click();
          await expect(modal.locator('.route-picker-row')).toHaveCount(2);
          await expect(modal.locator('.group-weight-input')).toHaveCount(2);
          await modal.locator('.group-weight-input').nth(0).fill('2');
          await modal.locator('.group-weight-input').nth(1).fill('1');
          await expect(modal.getByRole('button', {{ name: /^Save enabled$/ }})).toBeEnabled();
          await modal.getByRole('button', {{ name: /^Save enabled$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/, {{ timeout: 15000 }});
          expect(payload).toBeTruthy();
          await page.unroute('**/admin/routes');

          const row = page.locator('.route-list-table tbody tr').filter({{ hasText: 'audit-model-group' }}).first();
          await expect(row).toContainText('Model Group');
          await expect(row).toContainText('weighted_round_robin');
          await expect(row).toContainText('2 endpoints');
          await row.getByRole('button', {{ name: /^Edit$/ }}).click();
          const editModal = page.locator('#modal-overlay .modal').last();
          await expect(editModal).toContainText('Edit model group');
          await expect(editModal.locator('.route-picker-row')).toHaveCount(2);
          await expect(editModal.locator('.group-weight-input')).toHaveCount(2);
          await editModal.getByRole('button', {{ name: /^Cancel$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/);
        }}

        async function auditProviderTaskChoices(page) {{
          const modal = await openAddModel(page);
          const selects = modal.locator('select');
          await selects.nth(0).selectOption('rerank');
          const providerSelect = selects.nth(1);
          for (const key of ['jina', 'voyage', 'cohere', 'qwen']) {{
            await expect(providerSelect.locator('option[value="' + key + '"]')).toHaveCount(1);
          }}
          await providerSelect.selectOption('qwen');
          await expect(modal.locator('input').nth(0)).toHaveAttribute('placeholder', /workspace/);
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
        }}

        async function auditTeamAndKeyUi(page) {{
          await nav(page, 'teams', 'Teams');
          await page.getByRole('button', {{ name: /^New team$/ }}).click();
          let modal = page.locator('#modal-overlay .modal').last();
          await expect(modal).toContainText('Create team');
          await modal.locator('input').first().fill('Audit Team');
          await modal.getByRole('button', {{ name: /^Create$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/, {{ timeout: 15000 }});
          await expect(page.locator('#content')).toContainText('Audit Team');
          const teamRows = await page.locator('.team-list-table tbody tr').count();
          if (teamRows !== 2) throw new Error('expected seeded team + Audit Team, got team rows=' + teamRows);

          const row = page.locator('.team-list-table tbody tr').filter({{ hasText: 'Audit Team' }}).first();
          await row.getByRole('button', {{ name: /^Edit$/ }}).click();
          modal = page.locator('#modal-overlay .modal').last();
          await modal.locator('input').first().fill('Audit Team Renamed');
          await modal.getByRole('button', {{ name: /^Save$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/, {{ timeout: 15000 }});
          await expect(page.locator('#content')).toContainText('Audit Team Renamed');

          await nav(page, 'keys', 'API Keys');
          await page.getByRole('button', {{ name: /^New key$/ }}).click();
          modal = page.locator('#modal-overlay .modal').last();
          await expect(modal).toContainText('Create API key');
          const teamSelect = modal.locator('select').first();
          const auditTeamValue = await teamSelect.locator('option').filter({{ hasText: 'Audit Team Renamed' }}).first().getAttribute('value');
          if (!auditTeamValue) throw new Error('Audit Team Renamed option not found');
          await teamSelect.selectOption(auditTeamValue);
          await modal.locator('input').first().fill('audit@example.com');
          await modal.getByRole('button', {{ name: /^Create$/ }}).click();
          modal = page.locator('#modal-overlay .modal').last();
          await expect(modal).toContainText('Key created', {{ timeout: 15000 }});
          await expect(modal.locator('.key-reveal')).toContainText(/sk-/);
          await modal.getByRole('button', {{ name: /^Done$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/);
          await expect(page.locator('#content')).toContainText('audit@example.com');
        }}

        async function auditProviderEndpointUi(page) {{
          await nav(page, 'providers', 'Providers');
          await page.getByRole('button', {{ name: /Advanced endpoint/i }}).click();
          const modal = page.locator('#modal-overlay .modal').last();
          await expect(modal).toBeVisible();
          await expect(modal).toContainText(/endpoint|Provider/i);
          await modal.getByRole('button', {{ name: /^Cancel$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/);
        }}

        async function auditRoutes(page) {{
          await createCustomRoute(page, 'chat', 'mock-model', 'audit-chat');
          await createCustomRoute(page, 'chat', 'mock-model', 'audit-chat-b', mockURL + '/');
          await createCustomRoute(page, 'embedding', 'mock-embedding', 'audit-embedding');
          await createCustomRoute(page, 'rerank', 'mock-rerank', 'audit-rerank');
          await createCustomRoute(page, 'asr', 'mock-asr', 'audit-asr');
          await createModelGroupRoute(page);

          await page.reload({{ waitUntil: 'domcontentloaded' }});
          await expect(page.locator('#app-view')).toBeVisible();
          await nav(page, 'models', 'Models');
          for (const name of ['audit-chat', 'audit-chat-b', 'audit-embedding', 'audit-rerank', 'audit-asr', 'audit-model-group']) {{
            await expect(page.locator('#content')).toContainText(name);
          }}

          const routeRow = page.locator('.route-list-table tbody tr').filter({{ hasText: 'audit-chat' }}).first();
          await routeRow.getByRole('button', {{ name: /^Edit$/ }}).click();
          let modal = page.locator('#modal-overlay .modal').last();
          await expect(modal).toContainText('Edit model');
          await modal.locator('input').nth(3).fill('audit-chat-renamed');
          await modal.getByRole('button', {{ name: /^Test connection$/ }}).click();
          await expect(modal.locator('.connection-status')).toContainText('Connected', {{ timeout: 15000 }});
          await modal.getByRole('button', {{ name: /^Save enabled$/ }}).click();
          await expect(page.locator('#modal-overlay')).toHaveClass(/hidden/, {{ timeout: 15000 }});
          await expect(page.locator('#content')).toContainText('audit-chat-renamed');
        }}

        async function auditProviderKeyFilePath() {{
          const backends = await adminFetch('/admin/backends');
          const backend = backends.find(b => b.name === 'custom-llm') || backends[0];
          const routeName = 'audit-provider-key-file';
          await adminFetch('/admin/routes', 'POST', {{
            model_name: routeName,
            backend_ids: [backend.id],
            provider_model_name: 'mock-model',
            enabled: false,
            auth_mode: 'bearer',
            protocol: 'openai_chat',
            provider_key: 'sk-playwright-provider-key-file-test',
            first_byte_timeout: 180,
          }});
          const routes = await adminFetch('/admin/routes');
          const route = routes.find(r => r.model_name === routeName);
          if (!route) throw new Error('provider-key file route not saved');
          await adminFetch('/admin/routes/' + encodeURIComponent(routeName), 'DELETE');
        }}

        async function run() {{
          const browser = await chromium.launch({{ executablePath: process.env.PLAYWRIGHT_CHROME_EXECUTABLE, headless: true, args: ['--no-sandbox'] }});
          const page = await browser.newPage({{ viewport: {{ width: 1440, height: 1000 }} }});
          attachPageDiagnostics(page);
          page.on('dialog', dialog => dialog.accept());
          try {{
            await login(page);
            await auditNavigation(page);
            await auditProviderEndpointUi(page);
            await auditProviderTaskChoices(page);
            await auditRoutes(page);
            await auditTeamAndKeyUi(page);
            await auditProviderKeyFilePath();
            await nav(page, 'usage', 'Usage');
            await nav(page, 'settings', 'Settings');
          }} finally {{
            await browser.close();
          }}
        }}

        run().then(() => {{
          console.log('RESULT PASS full portal audit');
        }}).catch((err) => {{
          console.error(err && err.stack ? err.stack : err);
          process.exit(1);
        }});
        """
    )


def main() -> int:
    if os.environ.get("BRIGHTO_SKIP_RELEASE_BUILD") != "1":
        print("Building release binaries for full portal audit")
        smoke.sh("cargo", "build", "--release", "--locked", env=dict(os.environ))
    smoke.playwright_spec = full_playwright_spec
    return smoke.main()


if __name__ == "__main__":
    raise SystemExit(main())
