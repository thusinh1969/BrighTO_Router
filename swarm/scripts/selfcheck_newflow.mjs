import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const CLIENT_KEY = process.env.BRIGHTO_CLIENT_KEY || 'lc-0123456789abcdef0123456789abcdef';
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();

const result = { result: 'FAIL', base: BASE, failures: [], passes: [], evidence: {}, consoleErrors: [] };
function pass(a, s, e = {}) { console.log('[PASS]', a + ':', s); result.passes.push({ area: a, summary: s, evidence: e }); }
function fail(a, s, e = {}, f = '') { console.log('[FAIL]', a + ':', s); result.failures.push({ area: a, summary: s, evidence: e, requiredFix: f }); }

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
  if (!ADMIN) { fail('env', 'BRIGHTO_ADMIN_KEY required'); return; }
  const launch = { headless: true };
  if (process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE) launch.executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE;
  const browser = await chromium.launch(launch);
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 900 } });
  page.on('console', (m) => { if (m.type() === 'error') result.consoleErrors.push(m.text()); });
  page.on('pageerror', (e) => result.consoleErrors.push('pageerror: ' + e.message));
  page.on('dialog', async (d) => { result.evidence.lastDialog = d.message(); await d.accept(); });

  let routeCreated = false, backendCreatedId = null;
  try {
    await page.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 20000 });
    await page.locator('#login-user').fill('admin');
    await page.locator('#login-pass').fill(ADMIN);
    await page.evaluate(() => login());
    await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
    pass('auth', 'admin login works');

    const catalog = await adminFetch('/admin/provider-catalog');
    result.evidence.catalog = catalog;
    if (catalog.length >= 2 && catalog.some((c) => c.key === 'custom') && catalog.every((c) => c.dialect === 'openai' || c.dialect === 'anthropic')) {
      pass('catalog', 'provider catalog loaded from .env with 2 dialects + Custom LLM', { count: catalog.length });
    } else { fail('catalog', 'catalog wrong', catalog); }

    await page.locator('.nav[data-view="models"]').click({ force: true });
    await page.waitForTimeout(600);
    await page.getByRole('button', { name: 'Add model' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    const labels = await page.locator('.modal label').evaluateAll((ls) => ls.map((l) => l.textContent.trim()));
    result.evidence.addModelLabels = labels;
    const want = ['Provider', 'Base URL', 'API key', 'Provider model', 'Public model name (shown to clients)'];
    const missing = want.filter((w) => !labels.some((l) => l.includes(w)));
    if (!missing.length) pass('wizard', 'unified Add-model fields present', { labels });
    else fail('wizard', 'missing fields', { missing, labels });

    // select Custom LLM
    await modalField(page, 'Provider').selectOption('custom');
    await page.waitForTimeout(200);
    await modalField(page, 'Base URL').fill('http://127.0.0.1:9000/v1');
    await page.getByRole('button', { name: 'Load models' }).click({ force: true });
    // picker is a separate fixed overlay, not .modal
    await page.waitForTimeout(1200);
    const pickerText = await page.locator('body').innerText();
    if (pickerText.includes('Select a model')) pass('wizard', 'Load models opens a picker window');
    else fail('wizard', 'picker window did not open', { excerpt: pickerText.slice(0, 300) });
    await page.locator('text=mock-model').first().click();
    await page.getByRole('button', { name: 'Use this model' }).click();
    await page.waitForTimeout(400);
    const pmodel = await modalField(page, 'Provider model').inputValue();
    if (pmodel === 'mock-model') pass('wizard', 'picked model filled provider model field');
    else fail('wizard', 'provider model field wrong', { pmodel });

    await page.getByRole('button', { name: 'Test connection' }).click({ force: true });
    await page.waitForTimeout(1500);
    const statText = await page.locator('.modal').innerText();
    const saveBtn = page.getByRole('button', { name: 'Save model' });
    const saveEnabled = await saveBtn.isEnabled();
    if (statText.includes('Connected') && saveEnabled) pass('wizard', 'test connection passed and Save enabled', { excerpt: statText.slice(-200) });
    else fail('wizard', 'test connection did not enable Save', { excerpt: statText.slice(-200), saveEnabled });

    await saveBtn.click({ force: true });
    await page.locator('.modal').waitFor({ state: 'hidden', timeout: 10000 }).catch(() => {});
    await row(page, 'mock-model').waitFor({ state: 'visible', timeout: 10000 });
    const routes = await adminFetch('/admin/routes');
    const route = routes.find((r) => r.model_name === 'mock-model');
    const backends = await adminFetch('/admin/backends');
    const backend = backends.find((b) => b.base_url === 'http://127.0.0.1:9000/v1');
    backendCreatedId = backend?.id;
    if (route && route.provider_model_name === 'mock-model' && route.protocol === 'local_openai_chat' && route.auth_mode === 'none') {
      pass('save', 'model saved with auto-created local endpoint (auth none)', { route, backend });
      routeCreated = true;
    } else fail('save', 'model/backend not persisted correctly', { route, backend });

    // smoke through router with client key
    const resp = await fetch(BASE + '/v1/chat/completions', { method: 'POST', headers: { 'content-type': 'application/json', authorization: 'Bearer ' + CLIENT_KEY }, body: JSON.stringify({ model: 'mock-model', messages: [{ role: 'user', content: 'Reply OK' }], max_tokens: 8, stream: false }) });
    const body = await resp.text();
    if (resp.status === 200) pass('smoke', 'client call through router returns 200', { status: resp.status });
    else fail('smoke', 'client call failed', { status: resp.status, body: body.slice(0, 200) });

    if (result.consoleErrors.length) fail('runtime', 'console errors', { consoleErrors: result.consoleErrors });
  } catch (e) {
    fail('audit', 'crashed', { error: e.stack || e.message });
  } finally {
    try { await adminFetch('/admin/routes/mock-model', 'DELETE'); } catch {}
    if (backendCreatedId) { try { await adminFetch('/admin/backends/' + backendCreatedId, 'DELETE'); } catch {} }
    await browser.close().catch(() => {});
  }
}
await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(OUT + '/summary.json', JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
