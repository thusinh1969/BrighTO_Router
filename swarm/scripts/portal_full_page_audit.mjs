import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const stamp = Date.now().toString().slice(-7);
const prefix = `pw-fullvisual-${stamp}`;
const result = { result: 'FAIL', base: BASE, prefix, failures: [], pages: {}, screenshots: [], consoleErrors: [] };
function fail(summary, evidence = {}) { result.failures.push({ summary, evidence }); }

async function gotoWithRetry(page, url, attempts = 3) {
  let lastError = null;
  for (let i = 0; i < attempts; i++) {
    try {
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 20000 });
      return;
    } catch (e) {
      lastError = e;
      if (!/ERR_NETWORK_CHANGED|ERR_CONNECTION_RESET|ERR_HTTP2_PROTOCOL_ERROR/i.test(String(e && (e.message || e)))) break;
      await page.waitForTimeout(500 + i * 500);
    }
  }
  throw lastError;
}

async function adminFetch(path, method = 'GET', body) {
  const res = await fetch(BASE + path, { method, headers: { 'content-type': 'application/json', 'x-admin-key': ADMIN }, body: body === undefined ? undefined : JSON.stringify(body) });
  const text = await res.text();
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status} ${text.slice(0, 200)}`);
  return res.status === 204 || !text ? null : JSON.parse(text);
}
async function clientFetch(path, key, body) {
  const res = await fetch(BASE + path, { method: 'POST', headers: { 'content-type': 'application/json', authorization: 'Bearer ' + key }, body: JSON.stringify(body) });
  return { status: res.status, text: await res.text() };
}
async function seedDemo() {
  const backend = await adminFetch('/admin/backends', 'POST', { name: `${prefix}-Custom LLM`, base_url: 'http://127.0.0.1:9000/v1', api_key_ref: 'env:NONE', weight: 1, max_inflight: 0, format: 'openai', enabled: true });
  await adminFetch('/admin/routes', 'POST', { model_name: `${prefix}-public-model-with-long-but-realistic-name`, backend_ids: [backend.id], provider_model_name: 'mock-model', auth_mode: 'none', protocol: 'local_openai_chat', enabled: true, chars_per_token: 4, first_byte_timeout: 180, context_tokens: 1000000, max_output_tokens: 8192, price_input_per_mtok_usd: 0.14, price_output_per_mtok_usd: 0.28 });
  const team = await adminFetch('/admin/teams', 'POST', { name: `${prefix}-Team`, budget: { period: 'month', max_tokens: 1000000, per_model: {} }, enabled: true });
  const key = await adminFetch('/admin/keys', 'POST', { team_id: team.id, owner: `${prefix}-owner`, budget: null, expires_at: null });
  const revealed = await adminFetch('/admin/keys/' + key.id + '/reveal');
  await clientFetch('/v1/chat/completions', revealed.key, { model: `${prefix}-public-model-with-long-but-realistic-name`, messages: [{ role: 'user', content: 'Reply OK' }], max_tokens: 8, stream: false });
  for (let i = 0; i < 10; i++) {
    const rows = await adminFetch('/admin/usage?days=7');
    if (rows.some((r) => String(r.model || '').startsWith(prefix))) break;
    await new Promise((r) => setTimeout(r, 500));
  }
}
async function login(page) {
  await gotoWithRetry(page, BASE + '/');
  await page.locator('#login-user').fill('admin');
  await page.locator('#login-pass').fill(ADMIN);
  await page.evaluate(() => login());
  await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
}
async function go(page, view, mobile) {
  if (mobile) {
    await page.locator('.hamburger').click({ force: true });
    await page.waitForTimeout(250);
  }
  await page.locator(`.nav[data-view="${view}"]`).click({ force: true });
  await page.waitForTimeout(700);
}
async function inspectPage(page, label) {
  return page.evaluate(() => {
    const panelCount = document.querySelectorAll('.panel').length;
    const cards = document.querySelectorAll('.card').length;
    const summaryCards = [...document.querySelectorAll('.summary-card')].map((n) => (n.innerText || n.textContent || '').trim());
    const text = document.querySelector('#content')?.innerText || '';
    const tableStats = [...document.querySelectorAll('#content .table-wrap')].map((wrap) => {
      const table = wrap.querySelector('table');
      const rows = table ? [...table.querySelectorAll('tbody tr')] : [];
      const cells = table ? [...table.querySelectorAll('tbody td')] : [];
      const labelledCells = cells.filter((td) => (td.getAttribute('data-label') || '').trim()).length;
      const rect = wrap.getBoundingClientRect();
      return {
        rows: rows.length,
        cells: cells.length,
        labelledCells,
        wrapWidth: Math.round(rect.width),
        scrollWidth: wrap.scrollWidth,
        clientWidth: wrap.clientWidth,
        tableDisplay: table ? getComputedStyle(table).display : '',
        bodyDisplay: table ? getComputedStyle(table.querySelector('tbody') || table).display : '',
      };
    });
    const legendStats = [...document.querySelectorAll('#content .legend span')].map((node) => ({
      text: (node.innerText || node.textContent || '').trim(),
      scrollWidth: node.scrollWidth,
      clientWidth: node.clientWidth,
      scrollHeight: node.scrollHeight,
      clientHeight: node.clientHeight,
    }));
    return {
      title: document.querySelector('#page-title')?.textContent || '',
      panelCount,
      cards,
      summaryCards,
      bodyScrollWidth: document.body.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
      contentText: text.slice(0, 1200),
      tableStats,
      legendStats,
      visibleButtons: [...document.querySelectorAll('button')].filter((b) => b.offsetParent !== null).map((b) => b.innerText.trim()).filter(Boolean).slice(0, 30),
    };
  });
}
async function runViewport(browser, name, width, height) {
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width, height } });
  page.on('console', (m) => { if (m.type() === 'error' && !/ERR_NETWORK_CHANGED/i.test(m.text())) result.consoleErrors.push(`${name}: ${m.text()}`); });
  page.on('pageerror', (e) => result.consoleErrors.push(`${name}: pageerror ${e.message}`));
  await login(page);
  const mobile = width <= 820;
  result.pages[name] = {};
  for (const view of ['dashboard', 'providers', 'models', 'teams', 'keys', 'usage', 'settings']) {
    await go(page, view, mobile);
    const shot = `${OUT}/${name}-${view}.png`;
    await page.screenshot({ path: shot, fullPage: true });
    result.screenshots.push(shot);
    const metrics = await inspectPage(page, `${name}-${view}`);
    result.pages[name][view] = metrics;
    if (metrics.bodyScrollWidth > metrics.clientWidth + 8) fail(`${name}/${view}: whole-page horizontal overflow`, metrics);
    if (!metrics.title) fail(`${name}/${view}: missing page title`, metrics);
    if (!metrics.panelCount && view !== 'dashboard') fail(`${name}/${view}: no content panels`, metrics);
    if (['providers', 'models', 'teams', 'keys'].includes(view) && (metrics.summaryCards || []).length < 4) fail(`${name}/${view}: missing operational summary cards`, metrics);
    const clippedLegends = (metrics.legendStats || []).filter((l) => l.text && (l.scrollWidth > l.clientWidth + 4 || l.scrollHeight > l.clientHeight + 4));
    if (clippedLegends.length) fail(`${name}/${view}: chart legend text is clipped`, { clippedLegends, metrics });
    if (mobile) {
      const wideTables = metrics.tableStats.filter((t) => t.scrollWidth > t.clientWidth + 8);
      if (wideTables.length) fail(`${name}/${view}: mobile table still scrolls horizontally`, { wideTables, metrics });
      const rowTablesWithoutLabels = metrics.tableStats.filter((t) => t.rows > 0 && t.cells > 0 && t.labelledCells < Math.max(1, t.cells - t.rows));
      if (rowTablesWithoutLabels.length) fail(`${name}/${view}: mobile table rows lack readable column labels`, { rowTablesWithoutLabels, metrics });
      const tableModeFailures = metrics.tableStats.filter((t) => t.rows > 0 && (t.tableDisplay !== 'block' || t.bodyDisplay !== 'grid'));
      if (tableModeFailures.length) fail(`${name}/${view}: mobile tables are not rendered as stacked cards`, { tableModeFailures, metrics });
    }
  }
  await page.close();
}
async function main() {
  if (!ADMIN) { fail('BRIGHTO_ADMIN_KEY is required'); return; }
  await seedDemo();
  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  const browser = await chromium.launch(launch);
  try {
    await runViewport(browser, 'desktop-1440', 1440, 1000);
    await runViewport(browser, 'mobile-390', 390, 844);
  } finally {
    await browser.close().catch(() => {});
  }
  if (result.consoleErrors.length) fail('console errors during full-page audit', { consoleErrors: result.consoleErrors });
}
await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(`${OUT}/summary.json`, JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
