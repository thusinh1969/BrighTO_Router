import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const stamp = Date.now().toString().slice(-7);
const prefix = `pw-user-${stamp}`;
const result = { result: 'FAIL', base: BASE, prefix, failures: [], evidence: {}, screenshots: [], consoleErrors: [] };
function fail(summary, evidence = {}) { result.failures.push({ summary, evidence }); }

async function gotoWithRetry(page, url, attempts = 3) {
  let lastError = null;
  for (let i = 0; i < attempts; i++) {
    try { await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 20000 }); return; }
    catch (e) {
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
async function seedUser() {
  const backend = await adminFetch('/admin/backends', 'POST', { name: `${prefix}-Custom LLM`, base_url: 'http://127.0.0.1:9000/v1', api_key_ref: 'env:NONE', weight: 1, max_inflight: 0, format: 'openai', enabled: true });
  const model = `${prefix}-public-model`;
  await adminFetch('/admin/routes', 'POST', { model_name: model, backend_ids: [backend.id], provider_model_name: 'mock-model', auth_mode: 'none', protocol: 'local_openai_chat', enabled: true, chars_per_token: 4, first_byte_timeout: 180, context_tokens: 1000000, max_output_tokens: 8192, price_input_per_mtok_usd: 0.14, price_output_per_mtok_usd: 0.28 });
  const team = await adminFetch('/admin/teams', 'POST', { name: `${prefix}-Team`, budget: { period: 'month', max_tokens: 1000000, per_model: {} }, enabled: true });
  const key = await adminFetch('/admin/keys', 'POST', { team_id: team.id, owner: `${prefix}-owner`, allowed_models: [model], budget: null, expires_at: null });
  const revealed = await adminFetch('/admin/keys/' + key.id + '/reveal');
  const resp = await clientFetch('/v1/chat/completions', revealed.key, { model, messages: [{ role: 'user', content: 'Reply OK' }], max_tokens: 8, stream: false });
  if (resp.status !== 200) fail('seed request through user key failed', { status: resp.status, body: resp.text.slice(0, 180) });
  for (let i = 0; i < 10; i++) {
    const rows = await adminFetch('/admin/usage?days=7');
    if (rows.some((r) => r.model === model && Number(r.key_id) === Number(key.id))) break;
    await new Promise((r) => setTimeout(r, 500));
  }
  return { backendId: backend.id, teamId: team.id, keyId: key.id, key: revealed.key, prefix: key.prefix, model };
}
async function loginUser(page, key) {
  await gotoWithRetry(page, BASE + '/');
  await page.locator('#seg-user').click({ force: true });
  await page.locator('#login-apikey').fill(key);
  await page.getByRole('button', { name: 'Sign in' }).click({ force: true });
  await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
  await page.getByRole('heading', { name: 'Dashboard' }).waitFor({ state: 'visible', timeout: 12000 });
}
async function waitUserView(page, view) {
  const expected = {
    dashboard: ['Your API access', 'Call endpoint', '/v1/chat/completions'],
    usage: ['Tokens by model', 'Request logs'],
    settings: ['Portal preferences', 'Session', 'Model scope'],
  }[view] || [];
  await page.waitForFunction((needles) => {
    const content = (document.querySelector('#content')?.innerText || '').toLowerCase();
    return needles.every((n) => content.includes(String(n).toLowerCase()));
  }, expected, { timeout: 12000 });
  await page.waitForTimeout(100);
}
async function nav(page, view, mobile) {
  if (mobile) { await page.locator('.hamburger').click({ force: true }); await page.waitForTimeout(200); }
  await page.locator(`.nav[data-view="${view}"]`).click({ force: true });
  await waitUserView(page, view);
}
async function inspect(page) {
  return page.evaluate(() => {
    const visibleNav = [...document.querySelectorAll('.nav')].filter((n) => getComputedStyle(n).display !== 'none').map((n) => (n.innerText || n.textContent || '').trim());
    const content = document.querySelector('#content')?.innerText || '';
    const title = document.querySelector('#page-title')?.textContent || '';
    return {
      title,
      desc: document.querySelector('#page-desc')?.textContent || '',
      visibleNav,
      content: content.slice(0, 2500),
      panelCount: document.querySelectorAll('#content .panel').length,
      cardCount: document.querySelectorAll('#content .card').length,
      callItems: [...document.querySelectorAll('.call-item')].map((n) => (n.innerText || n.textContent || '').trim()),
      callCodeStats: [...document.querySelectorAll('.call-item code')].map((n) => {
        const st = getComputedStyle(n);
        return {
          text: (n.innerText || n.textContent || '').trim(),
          scrollWidth: n.scrollWidth,
          clientWidth: n.clientWidth,
          scrollHeight: n.scrollHeight,
          clientHeight: n.clientHeight,
          whiteSpace: st.whiteSpace,
          overflowX: st.overflowX,
          textOverflow: st.textOverflow,
        };
      }),
      bodyScrollWidth: document.body.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    };
  });
}
async function runViewport(browser, seed, name, width, height) {
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width, height } });
  page.on('console', (m) => { if (m.type() === 'error' && !/ERR_NETWORK_CHANGED/i.test(m.text())) result.consoleErrors.push(`${name}: ${m.text()}`); });
  page.on('pageerror', (e) => result.consoleErrors.push(`${name}: pageerror ${e.message}`));
  const mobile = width <= 820;
  try {
    await loginUser(page, seed.key);
    await waitUserView(page, 'dashboard');
    const dashShot = `${OUT}/${name}-user-dashboard.png`;
    await page.screenshot({ path: dashShot, fullPage: true });
    result.screenshots.push(dashShot);
    const dash = await inspect(page);
    result.evidence[`${name}-dashboard`] = dash;
    if (!dash.visibleNav.includes('Dashboard') || !dash.visibleNav.includes('Usage') || !dash.visibleNav.includes('Settings')) fail(`${name}: user nav missing Dashboard/Usage/Settings`, dash);
    for (const forbidden of ['Providers', 'Models & Routes', 'Teams', 'API Keys']) {
      if (dash.visibleNav.includes(forbidden)) fail(`${name}: user nav exposes admin menu ${forbidden}`, dash);
    }
    if (!/Your API access and usage/i.test(dash.desc)) fail(`${name}: user dashboard topbar description is not role-aware`, dash);
    if (!/Call endpoint|POST|\/v1\/chat\/completions|Authorization: Bearer <your API key>/i.test(dash.content)) fail(`${name}: user dashboard missing call endpoint quick start`, dash);
    if (!dash.content.includes(seed.model)) fail(`${name}: user dashboard missing allowed/used model`, dash);
    if (dash.callItems.length < 4) fail(`${name}: user call endpoint panel missing fields`, dash);
    const clippedCallCodes = (dash.callCodeStats || []).filter((c) => c.scrollWidth > c.clientWidth + 4 || c.whiteSpace === 'nowrap' || c.textOverflow === 'ellipsis');
    if (clippedCallCodes.length) fail(`${name}: user call endpoint code is clipped`, { clippedCallCodes, dash });
    if (dash.bodyScrollWidth > dash.clientWidth + 8) fail(`${name}: user dashboard horizontal overflow`, dash);

    await nav(page, 'usage', mobile);
    const usageShot = `${OUT}/${name}-user-usage.png`;
    await page.screenshot({ path: usageShot, fullPage: true });
    result.screenshots.push(usageShot);
    const usage = await inspect(page);
    result.evidence[`${name}-usage`] = usage;
    if (!/Your requests and token usage/i.test(usage.desc)) fail(`${name}: user usage topbar description is not role-aware`, usage);
    if (!usage.content.includes(seed.model) || !/Tokens\/sec/i.test(usage.content)) fail(`${name}: user usage missing own model or Tokens/sec signal`, usage);
    if (/Provider health|By team|By API key/i.test(usage.content)) fail(`${name}: user usage leaked admin-only aggregations`, usage);
    if (usage.bodyScrollWidth > usage.clientWidth + 8) fail(`${name}: user usage horizontal overflow`, usage);

    await nav(page, 'settings', mobile);
    const settingsShot = `${OUT}/${name}-user-settings.png`;
    await page.screenshot({ path: settingsShot, fullPage: true });
    result.screenshots.push(settingsShot);
    const settings = await inspect(page);
    result.evidence[`${name}-settings`] = settings;
    if (!/Portal preferences and session/i.test(settings.desc)) fail(`${name}: user settings topbar description is not role-aware`, settings);
    if (!/Portal preferences|Font size|Density/i.test(settings.content)) fail(`${name}: user settings missing Portal preferences`, settings);
    if (!/Session|Model scope/i.test(settings.content) || !settings.content.includes(seed.prefix)) fail(`${name}: user settings missing key session summary`, settings);
    if (/Settings are admin-only|Router address|Database|Config reload/i.test(settings.content)) fail(`${name}: user settings leaks admin-only/runtime language`, settings);
    if (settings.bodyScrollWidth > settings.clientWidth + 8) fail(`${name}: user settings horizontal overflow`, settings);
  } finally {
    await page.close().catch(() => {});
  }
}
async function main() {
  if (!ADMIN) { fail('BRIGHTO_ADMIN_KEY is required'); return; }
  let seed = null;
  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  try {
    seed = await seedUser();
    result.evidence.seed = { keyPrefix: seed.prefix, model: seed.model, keyId: seed.keyId, teamId: seed.teamId, backendId: seed.backendId };
    const browser = await chromium.launch(launch);
    try {
      await runViewport(browser, seed, 'desktop-1440', 1440, 1000);
      await runViewport(browser, seed, 'mobile-390', 390, 844);
    } finally { await browser.close().catch(() => {}); }
  } catch (e) {
    fail('user journey audit crashed', { error: e.stack || e.message });
  }
  if (result.consoleErrors.length) fail('console errors during user journey audit', { consoleErrors: result.consoleErrors });
}
await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(`${OUT}/summary.json`, JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
