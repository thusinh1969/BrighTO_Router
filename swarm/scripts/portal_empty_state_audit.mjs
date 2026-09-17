import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const result = { result: 'FAIL', base: BASE, failures: [], evidence: {}, consoleErrors: [] };
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
async function login(page) {
  await gotoWithRetry(page, BASE + '/');
  await page.locator('#login-user').fill('admin');
  await page.locator('#login-pass').fill(ADMIN);
  await page.evaluate(() => login());
  await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
}
async function main() {
  if (!ADMIN) { fail('BRIGHTO_ADMIN_KEY is required'); return; }
  const routes = await adminFetch('/admin/routes');
  const backends = await adminFetch('/admin/backends');
  result.evidence.initialCounts = { routes: routes.length, backends: backends.length };
  if (routes.length !== 0 || backends.length !== 0) {
    fail('empty-state audit requires zero routes and zero providers before it runs', result.evidence.initialCounts);
    return;
  }
  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  const browser = await chromium.launch(launch);
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width: 1440, height: 1000 } });
  page.on('console', (m) => { if (m.type() === 'error' && !/ERR_NETWORK_CHANGED|ERR_CONNECTION_RESET|ERR_HTTP2_PROTOCOL_ERROR/i.test(m.text())) result.consoleErrors.push(m.text()); });
  page.on('pageerror', (e) => result.consoleErrors.push(`pageerror ${e.message}`));
  try {
    await login(page);
    await page.locator('.nav[data-view="dashboard"]').click({ force: true });
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${OUT}/empty-dashboard.png`, fullPage: true });
    const dash = await page.evaluate(() => ({
      text: document.querySelector('#content')?.innerText || '',
      buttons: [...document.querySelectorAll('#content button')].map((b) => b.innerText.trim()).filter(Boolean),
      launchSteps: document.querySelectorAll('.launch-step').length,
      launchPanels: [...document.querySelectorAll('#content .panel h3')].filter((h) => (h.textContent || '') === 'Launch checklist').length,
      addFirstModelButtons: [...document.querySelectorAll('#content button')].filter((b) => (b.innerText || '').trim() === 'Add first model').length,
      hasLaunch: !!document.querySelector('.launch-list'),
      emptyPanels: document.querySelectorAll('#content .empty').length,
      sectionTitles: [...document.querySelectorAll('#content h3, #content .diagnostic-details summary b')].map((n) => (n.textContent || '').trim()),
    }));
    result.evidence.dashboard = dash;
    if (!/Ready to set up/i.test(dash.text) || /Needs attention/i.test(dash.text)) fail('dashboard first-run hero should be onboarding, not an alarm state', dash);
    if (/priced routes ready/i.test(dash.text)) fail('dashboard first-run must not claim pricing is ready before any route exists', dash);
    if (!/add route pricing/i.test(dash.text) || !/created with model/i.test(dash.text)) fail('dashboard first-run readiness copy should explain the next setup actions', dash);
    if (!dash.hasLaunch || dash.launchSteps !== 3) fail('dashboard empty state must show a 3-step launch checklist', dash);
    if (dash.launchPanels !== 1) fail('dashboard empty state must not render duplicate launch panels', dash);
    if (dash.addFirstModelButtons !== 1) fail('dashboard empty state must show exactly one Add first model CTA', dash);
    if (!dash.buttons.includes('View API keys')) fail('dashboard empty state must include View API keys CTA', dash);
    if (dash.emptyPanels !== 0) fail('dashboard first-run should not render repeated empty chart/log panels below the launch checklist', dash);
    if (dash.sectionTitles.some((t) => ['Tokens by model — last 30 days', 'Top consumers', 'Top models', 'Recent requests', 'Technical diagnostics'].includes(t))) {
      fail('dashboard first-run should stop at Launch checklist instead of showing empty analytics sections', dash);
    }

    await page.getByRole('button', { name: 'Add first model' }).click({ force: true });
    await page.locator('#modal-overlay:not(.hidden) .modal').waitFor({ state: 'visible', timeout: 5000 });
    const modal = await page.evaluate(() => ({
      title: document.querySelector('#modal-overlay .modal h3')?.textContent || '',
      text: document.querySelector('#modal-overlay .modal')?.innerText || '',
    }));
    result.evidence.addFirstModelModal = { title: modal.title, text: modal.text.slice(0, 800) };
    if (!/Add model/i.test(modal.title)) fail('Add first model CTA must open Add model wizard', modal);
    if (!/Provider/i.test(modal.text) || !/API key/i.test(modal.text) || !/Test connection/i.test(modal.text)) fail('Add model wizard must expose provider/API key/test flow', modal);
    await page.keyboard.press('Escape').catch(() => {});
    await page.evaluate(() => closeModal());

    await page.locator('.nav[data-view="usage"]').click({ force: true });
    await page.waitForTimeout(600);
    await page.screenshot({ path: `${OUT}/empty-usage.png`, fullPage: true });
	    const usage = await page.evaluate(() => ({
	      text: document.querySelector('#content')?.innerText || '',
	      emptyCount: document.querySelectorAll('#content .empty').length,
	      emptyTables: [...document.querySelectorAll('#content table')].filter((t) => t.querySelectorAll('tbody tr').length === 0).length,
	      groupTitles: [...document.querySelectorAll('#content .panel h3')].map((x) => x.textContent || ''),
	      launchSteps: document.querySelectorAll('#content .launch-step').length,
	      addFirstModelButtons: [...document.querySelectorAll('#content button')].filter((b) => (b.innerText || '').trim() === 'Add first model').length,
	    }));
	    result.evidence.usage = usage;
	    if (!usage.text.includes('No usage data for this filter.')) fail('usage empty chart must explain no data', usage);
	    if (!usage.text.includes('No data for this filter yet.')) fail('usage empty group panels must show empty-state copy', usage);
	    if (!usage.text.includes('Launch checklist') || usage.launchSteps !== 3) fail('usage empty state must show launch checklist before repeated empty panels', usage);
	    if (usage.addFirstModelButtons !== 1) fail('usage empty state must include one Add first model CTA', usage);
	    if (usage.emptyTables > 0) fail('usage empty state must not render naked empty tables', usage);
  } finally {
    await browser.close().catch(() => {});
  }
  if (result.consoleErrors.length) fail('console errors during empty-state audit', { consoleErrors: result.consoleErrors });
}
await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(`${OUT}/summary.json`, JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
