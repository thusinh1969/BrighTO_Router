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
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 900 } });
  page.on('console', (m) => { if (m.type() === 'error' && !/ERR_NETWORK_CHANGED|ERR_CONNECTION_RESET|ERR_HTTP2_PROTOCOL_ERROR/i.test(m.text())) result.consoleErrors.push(m.text()); });
  page.on('pageerror', (e) => result.consoleErrors.push('pageerror: ' + e.message));
  page.on('dialog', async (d) => { result.evidence.lastDialog = d.message(); await d.accept(); });

  try {
    await page.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 20000 });
    await page.locator('#login-user').fill('admin');
    await page.locator('#login-pass').fill(ADMIN);
    await page.evaluate(() => login());
    await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
    pass('auth', 'admin login works');

    const catalog = await adminFetch('/admin/provider-catalog');
    result.evidence.catalog = catalog;
    if (catalog.length === 10 && catalog.some((c) => c.key === 'zai') && catalog.some((c) => c.key === 'meta-muse')) {
      pass('catalog', '10 providers incl Z.AI + Meta Muse', { count: catalog.length });
    } else { fail('catalog', 'catalog wrong', catalog.map((c) => c.key)); }
    const gemini = catalog.find((c) => c.key === 'gemini');
    if (gemini && gemini.enabled === false) pass('catalog', 'Gemini marked coming-soon'); else fail('catalog', 'Gemini not disabled', { gemini });

    await page.locator('.nav[data-view="models"]').click({ force: true });
    await page.waitForTimeout(600);
    await page.getByRole('button', { name: 'Add model' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    const labels = await page.locator('.modal label').evaluateAll((ls) => ls.map((l) => l.textContent.trim()));
    result.evidence.addModelLabels = labels;

    const saveEnabledBtn = page.getByRole('button', { name: 'Save enabled' });
    const saveDisabledBtn = page.getByRole('button', { name: 'Save draft' });
    if (await saveEnabledBtn.isDisabled()) pass('gating', 'Save enabled disabled before test');
    else fail('gating', 'Save enabled should be disabled before test');
    if (await saveDisabledBtn.isEnabled()) pass('gating', 'Save draft available without test');
    else fail('gating', 'Save draft should be available');

    await modalField(page, 'Provider').selectOption('custom-llm');
    await page.waitForTimeout(200);
    await modalField(page, 'Base URL').fill('http://127.0.0.1:9000/v1');
    await page.getByRole('button', { name: 'Load models' }).click({ force: true });
    await page.waitForTimeout(1200);
    const pickerText = await page.locator('body').innerText();
    if (pickerText.includes('Select a model')) pass('picker', 'Load models opens chooser'); else fail('picker', 'chooser missing', { excerpt: pickerText.slice(0, 200) });
    await page.locator('text=mock-model').first().click();
    await page.getByRole('button', { name: 'Use this model' }).click();
    await page.waitForTimeout(400);
    const pmodel = await modalField(page, 'Provider model').inputValue();
    if (pmodel === 'mock-model') pass('picker', 'picked model filled'); else fail('picker', 'model field wrong', { pmodel });

    await page.getByRole('button', { name: 'Test connection' }).click({ force: true });
    await page.waitForTimeout(1500);
    const statText = await page.locator('.modal').innerText();
    if (statText.includes('Connected') && (await saveEnabledBtn.isEnabled())) pass('gating', 'test PASS enables Save enabled', { excerpt: statText.slice(-160) });
    else fail('gating', 'test did not enable Save enabled', { excerpt: statText.slice(-160), enabled: await saveEnabledBtn.isEnabled() });

    await saveEnabledBtn.click({ force: true });
    await page.locator('.modal').waitFor({ state: 'hidden', timeout: 10000 }).catch(() => {});
    await row(page, 'mock-model').waitFor({ state: 'visible', timeout: 10000 });
    let routes = await adminFetch('/admin/routes');
    const route = routes.find((r) => r.model_name === 'mock-model');
    let backends = await adminFetch('/admin/backends');
    const backend = backends.find((b) => b.base_url === 'http://127.0.0.1:9000/v1');
    if (route && route.enabled === true && route.auth_mode === 'none' && route.protocol === 'local_openai_chat') pass('save', 'model saved enabled with auto-created connection', { route, backend });
    else fail('save', 'model/backend wrong', { route, backend });

    const resp = await fetch(BASE + '/v1/chat/completions', { method: 'POST', headers: { 'content-type': 'application/json', authorization: 'Bearer ' + CLIENT_KEY }, body: JSON.stringify({ model: 'mock-model', messages: [{ role: 'user', content: 'Reply OK' }], max_tokens: 8, stream: false }) });
    if (resp.status === 200) pass('smoke', 'client call returns 200', { status: resp.status });
    else fail('smoke', 'client call failed', { status: resp.status, body: (await resp.text()).slice(0, 200) });

    await page.getByRole('button', { name: 'Add model' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    await modalField(page, 'Provider').selectOption('custom-llm');
    await modalField(page, 'Base URL').fill('http://127.0.0.1:9000/v1');
    await modalField(page, 'Provider model').fill('mock-model');
    await modalField(page, 'Public model name (shown to clients)').fill('mock-model-2');
    await page.getByRole('button', { name: 'Test connection' }).click({ force: true });
    await page.waitForTimeout(1500);
    await page.getByRole('button', { name: 'Save enabled' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'hidden', timeout: 10000 }).catch(() => {});
    backends = await adminFetch('/admin/backends');
    const sameUrl = backends.filter((b) => b.base_url === 'http://127.0.0.1:9000/v1');
    if (sameUrl.length === 1) pass('dedup', 'one connection for repeated URL', { count: sameUrl.length });
    else fail('dedup', 'duplicate connection created', { count: sameUrl.length });

    if (result.consoleErrors.length) fail('runtime', 'console errors', { consoleErrors: result.consoleErrors });
  } catch (e) {
    fail('audit', 'crashed', { error: e.stack || e.message });
  } finally {
    try { await adminFetch('/admin/routes/mock-model', 'DELETE'); } catch {}
    try { await adminFetch('/admin/routes/mock-model-2', 'DELETE'); } catch {}
    const bs = await adminFetch('/admin/backends').catch(() => []);
    for (const b of (bs || [])) { if (b.base_url === 'http://127.0.0.1:9000/v1') { try { await adminFetch('/admin/backends/' + b.id, 'DELETE'); } catch {} } }
    await browser.close().catch(() => {});
  }
}
await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(OUT + '/summary.json', JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
