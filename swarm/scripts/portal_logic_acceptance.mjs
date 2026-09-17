import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const stamp = Date.now().toString().slice(-8);
const prefix = `pw-logic-${stamp}`;

const result = { result: 'FAIL', base: BASE, prefix, failures: [], passes: [], evidence: {}, consoleErrors: [] };
function pass(area, summary, evidence = {}) { console.log('[PASS] ' + area + ': ' + summary); result.passes.push({ area, summary, evidence }); }
function fail(area, summary, evidence = {}, requiredFix = '') { console.log('[FAIL] ' + area + ': ' + summary); result.failures.push({ area, summary, evidence, requiredFix }); }

async function adminFetch(path, method = 'GET', body) {
  const res = await fetch(BASE + path, { method, headers: { 'content-type': 'application/json', 'x-admin-key': ADMIN }, body: body === undefined ? undefined : JSON.stringify(body) });
  const text = await res.text();
  if (!res.ok) throw new Error(method + ' ' + path + ' -> ' + res.status + ' ' + text);
  return res.status === 204 || !text ? null : JSON.parse(text);
}
function modalField(page, label) {
  return page.locator('.modal .field').filter({ has: page.locator('label', { hasText: label }) }).locator('input,select,textarea').first();
}
function row(page, text) { return page.locator('tr').filter({ hasText: text }).first(); }

async function main() {
  if (!ADMIN) { fail('environment', 'BRIGHTO_ADMIN_KEY is required'); return; }
  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  const browser = await chromium.launch(launch);
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 900 } });
  page.on('console', (m) => { if (m.type() === 'error') result.consoleErrors.push(m.text()); });
  page.on('pageerror', (e) => result.consoleErrors.push('pageerror: ' + e.message));
  page.on('dialog', async (d) => { result.evidence.lastDialog = d.message(); await d.accept(); });

  const modelName = prefix + '-mock';
  const modelName2 = prefix + '-mock2';
  let teamId = null, keyId = null;

  try {
    await page.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 20000 });
    await page.locator('#login-user').fill('admin');
    await page.locator('#login-pass').fill(ADMIN);
    await page.evaluate(() => login());
    await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
    pass('auth', 'Admin login works over live HTTPS');

    const fmt = await page.evaluate(() => ({ fmt1000: fmt(1000), fmt50000: fmt(50000), fmt1m: fmt(1000000), density: document.documentElement.dataset.density }));
    if (fmt.fmt1000 === '1K' && fmt.fmt50000 === '50K' && fmt.fmt1m === '1M' && fmt.density === 'compact') pass('polish', 'compact formatter + density live', fmt);
    else fail('polish', 'formatter/density wrong', fmt);

    const catalog = await adminFetch('/admin/provider-catalog');
    if (catalog.length >= 8 && catalog.some((c) => c.key === 'custom-llm') && catalog.every((c) => c.dialect === 'openai' || c.dialect === 'anthropic')) {
      pass('catalog', 'provider catalog from .env (2 dialects + Custom LLM)', { count: catalog.length });
    } else fail('catalog', 'catalog wrong', catalog.map((c) => c.key));
    const gemini = catalog.find((c) => c.key === 'gemini');
    if (gemini && gemini.enabled === false) pass('catalog', 'Gemini coming-soon disabled'); else fail('catalog', 'Gemini not disabled', { gemini });

    await page.locator('.nav[data-view="models"]').click({ force: true });
    await page.waitForTimeout(600);
    await page.getByRole('button', { name: 'Add model' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    const saveEnabled = page.getByRole('button', { name: 'Save enabled' });
    const saveDraft = page.getByRole('button', { name: 'Save draft' });
    if (await saveEnabled.isDisabled()) pass('gating', 'Save enabled disabled before test'); else fail('gating', 'Save enabled should be disabled before test');
    if (await saveDraft.isEnabled()) pass('gating', 'Save draft available without test'); else fail('gating', 'Save draft should be available');

    await modalField(page, 'Provider').selectOption('custom-llm');
    await page.waitForTimeout(200);
    await modalField(page, 'Base URL').fill('http://127.0.0.1:9000/v1');
    await page.getByRole('button', { name: 'Load models' }).click({ force: true });
    const picker = page.locator('body > div').filter({ has: page.getByRole('heading', { name: /Select a model/ }) }).last();
    await picker.waitFor({ state: 'visible', timeout: 10000 });
    const bodyText = await picker.innerText();
    if (bodyText.includes('mock-model')) pass('models', 'Load models opens chooser'); else fail('models', 'chooser missing', { excerpt: bodyText.slice(0, 200) });
    await picker.locator('.mono', { hasText: 'mock-model' }).first().click({ force: true });
    await picker.getByRole('button', { name: 'Use this model' }).click({ force: true });
    await page.waitForTimeout(400);
    await modalField(page, 'Public model name (shown to clients)').fill(modelName);
    await page.getByRole('button', { name: 'Test connection' }).click({ force: true });
    await page.waitForTimeout(1500);
    const statText = await page.locator('.modal').innerText();
    if (statText.includes('Connected') && (await saveEnabled.isEnabled())) pass('models', 'test PASS enables Save enabled', { excerpt: statText.slice(-160) });
    else fail('models', 'test did not enable Save enabled', { excerpt: statText.slice(-160) });

    await saveEnabled.click({ force: true });
    await page.locator('.modal').waitFor({ state: 'hidden', timeout: 10000 }).catch(() => {});
    await row(page, modelName).waitFor({ state: 'visible', timeout: 10000 });
    let routes = await adminFetch('/admin/routes');
    const route = routes.find((r) => r.model_name === modelName);
    let backends = await adminFetch('/admin/backends');
    const backend = backends.find((b) => b.base_url === 'http://127.0.0.1:9000/v1');
    if (route && route.enabled === true && route.effective_enabled === true && backend) pass('models', 'model saved enabled with auto-created connection', { route, backend });
    else fail('models', 'model/connection wrong', { route, backend });
    if (route && route.auth_mode === 'none' && route.protocol === 'local_openai_chat') {
      pass('models', 'Custom LLM blank key saves as no-auth local route', { auth_mode: route.auth_mode, protocol: route.protocol });
    } else {
      fail('models', 'Custom LLM blank key saved wrong auth/protocol', { route }, 'For Custom LLM local/private URL, blank wizard API key must save auth_mode=none and protocol=local_openai_chat, even if CUSTOM_LLM_API_KEY exists in .env.');
    }

    await page.getByRole('button', { name: 'Add model' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    await modalField(page, 'Provider').selectOption('custom-llm');
    await modalField(page, 'Base URL').fill('http://127.0.0.1:9000/v1');
    await modalField(page, 'Provider model').fill('mock-model');
    await modalField(page, 'Public model name (shown to clients)').fill(modelName2);
    await page.getByRole('button', { name: 'Test connection' }).click({ force: true });
    await page.waitForTimeout(1500);
    await page.getByRole('button', { name: 'Save enabled' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'hidden', timeout: 10000 }).catch(() => {});
    backends = await adminFetch('/admin/backends');
    const sameUrl = backends.filter((b) => b.base_url === 'http://127.0.0.1:9000/v1');
    if (sameUrl.length === 1) pass('models', 'dedup: one connection for repeated URL', { count: sameUrl.length });
    else fail('models', 'duplicate connection created', { count: sameUrl.length });

    const reveal = await adminFetch('/admin/keys/1/reveal').catch(() => null);
    const clientKey = reveal ? reveal.key : 'lc-0123456789abcdef0123456789abcdef';
    const resp = await fetch(BASE + '/v1/chat/completions', { method: 'POST', headers: { 'content-type': 'application/json', authorization: 'Bearer ' + clientKey }, body: JSON.stringify({ model: modelName, messages: [{ role: 'user', content: 'Reply OK' }], max_tokens: 8, stream: false }) });
    if (resp.status === 200) pass('smoke', 'client call returns 200', { status: resp.status });
    else fail('smoke', 'client call failed', { status: resp.status, body: (await resp.text()).slice(0, 200) });

    // Poll until the async ledger records the smoke usage.
    let usedRoute = null;
    for (let i = 0; i < 8; i++) {
      await new Promise((r) => setTimeout(r, 500));
      routes = await adminFetch('/admin/routes');
      usedRoute = routes.find((r) => r.model_name === modelName);
      if (usedRoute && usedRoute.usage_count > 0) break;
    }
    if (usedRoute && usedRoute.usage_count > 0 && usedRoute.can_delete === false) pass('lifecycle', 'model with usage has can_delete=false', { usage_count: usedRoute.usage_count });
    else fail('lifecycle', 'model usage guard missing', { usedRoute });
    const delRes = await fetch(BASE + '/admin/routes/' + encodeURIComponent(modelName), { method: 'DELETE', headers: { 'x-admin-key': ADMIN } });
    if (delRes.status === 409) pass('lifecycle', 'delete model with usage returns 409'); else fail('lifecycle', 'delete model with usage should 409', { status: delRes.status });

    await page.locator('.nav[data-view="providers"]').click({ force: true });
    await page.waitForTimeout(600);
    const testBackend = (await adminFetch('/admin/backends')).find((b) => b.base_url === 'http://127.0.0.1:9000/v1');
    if (testBackend && testBackend.can_delete === false) pass('lifecycle', 'in-use connection API can_delete=false', { backend: testBackend });
    else fail('lifecycle', 'in-use connection API should have can_delete=false', { backend: testBackend });
    const connRow = page.locator('tr').filter({ hasText: '127.0.0.1:9000/v1' }).first();
    if (await connRow.count()) {
      const delBtn = connRow.getByRole('button', { name: 'Delete' });
      if (await delBtn.isDisabled()) pass('lifecycle', 'in-use connection Delete disabled');
      else fail('lifecycle', 'in-use connection Delete should be disabled', { rowText: await connRow.innerText() });

      await connRow.getByRole('button', { name: 'Route' }).click({ force: true });
      await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
      const routeFromProvider = {
        text: (await page.locator('.modal').innerText()).slice(0, 1200),
        baseUrl: await modalField(page, 'Base URL').inputValue(),
        stepCount: await page.locator('.modal .wizard-step').count(),
      };
      if (routeFromProvider.baseUrl === 'http://127.0.0.1:9000/v1' && routeFromProvider.stepCount === 3 && /Test connection/.test(routeFromProvider.text)) {
        pass('providers', 'Route button opens guided Add model wizard with connection prefilled', routeFromProvider);
      } else {
        fail('providers', 'Route button wizard is wrong', routeFromProvider);
      }
      await page.evaluate(() => closeModal());
    } else {
      fail('lifecycle', 'test connection row not visible in Connections UI', { base_url: 'http://127.0.0.1:9000/v1' });
    }

    const team = await adminFetch('/admin/teams', 'POST', { name: prefix + '-team', budget: { period: 'month', max_tokens: 1000000, per_model: {} }, enabled: true });
    teamId = team.id;
    await adminFetch('/admin/teams/' + teamId, 'PATCH', { budget: null });
    const teamsAfter = await adminFetch('/admin/teams');
    const trow = teamsAfter.find((t) => t.id === teamId);
    if (trow && trow.budget === null) pass('teams', 'explicit budget:null clears team budget', { api: trow });
    else fail('teams', 'budget:null not cleared', { api: trow });

    await page.locator('.nav[data-view="keys"]').click({ force: true });
    await page.waitForTimeout(600);
    let modalOpened = false;
    for (let attempt = 0; attempt < 4 && !modalOpened; attempt++) {
      await page.getByRole('button', { name: 'New key' }).click({ force: true }).catch(() => {});
      await page.waitForTimeout(400);
      modalOpened = await page.locator('.modal').isVisible().catch(() => false);
    }
    const modalState = await page.evaluate(() => ({ overlayClass: document.querySelector('#modal-overlay') ? document.querySelector('#modal-overlay').className : 'NO-OVERLAY', children: document.querySelector('#modal-overlay') ? document.querySelector('#modal-overlay').children.length : -1, modal: !!document.querySelector('.modal') }));
    result.evidence.keyModalState = modalState;
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    await modalField(page, 'Team').selectOption('1');
    await modalField(page, 'Owner').fill(prefix + '-owner');
    await page.locator('.modal').getByRole('button', { name: 'Create' }).click({ force: true });
    await page.locator('.modal').filter({ hasText: 'Key created' }).waitFor({ state: 'visible', timeout: 10000 });
    const keyText = await page.locator('.modal').innerText();
    const keyMatch = keyText.match(/lc-[A-Za-z0-9._-]+/);
    await page.locator('.modal').getByRole('button', { name: 'Done' }).click({ force: true });
    await row(page, prefix + '-owner').waitFor({ state: 'visible', timeout: 10000 });
    const key = (await adminFetch('/admin/keys')).find((k) => k.owner === prefix + '-owner');
    keyId = key?.id;
    if (keyMatch && key && key.revealable) pass('keys', 'key create + revealable', { owner: key.owner, prefix: key.prefix });
    else fail('keys', 'key create/reveal missing', { key });

    const userKey = (await adminFetch('/admin/keys')).find((k) => k.enabled && k.revealable !== false && k.id !== keyId) || key;
    if (userKey) {
      const ur = await adminFetch('/admin/keys/' + userKey.id + '/reveal');
      await page.getByRole('button', { name: 'Sign out' }).click({ force: true });
      await page.locator('#seg-user').click({ force: true });
      await page.locator('#login-apikey').fill(ur.key);
      await page.getByRole('button', { name: 'Sign in' }).click({ force: true });
      await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 10000 });
      const userState = await page.evaluate(() => ({ providers: getComputedStyle(document.querySelector('#nav-providers')).display, models: getComputedStyle(document.querySelector('#nav-models')).display, keys: getComputedStyle(document.querySelector('#nav-keys')).display, title: document.querySelector('#page-title')?.textContent }));
      if (userState.providers === 'none' && userState.models === 'none' && userState.keys === 'none' && userState.title === 'Dashboard') pass('user', 'user login hides admin menus + Dashboard', userState);
      else fail('user', 'user login state wrong', userState);
    }

    if (result.consoleErrors.length) fail('runtime', 'console errors', { consoleErrors: result.consoleErrors });
  } catch (e) {
    fail('audit', 'gate crashed', { error: e.stack || e.message });
  } finally {
    try { await adminFetch('/admin/keys/' + keyId, 'DELETE'); } catch {}
    await browser.close().catch(() => {});
  }
}

await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(OUT + '/summary.json', JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
