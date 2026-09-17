import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const result = { result: 'FAIL', base: BASE, failures: [], pages: {}, screenshots: [] };
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

async function inspect(page) {
  return page.evaluate(() => {
    const login = document.querySelector('#login-view');
    const adminFields = document.querySelector('#admin-fields');
    const userField = document.querySelector('#user-field');
    const passLabel = document.querySelector('#admin-fields .field:nth-child(2) label');
    const userLabel = document.querySelector('#user-field label');
    const pass = document.querySelector('#login-pass');
    const api = document.querySelector('#login-apikey');
    const signIn = [...document.querySelectorAll('button')].find((b) => /sign in/i.test(b.innerText || ''));
    function visible(n) { return !!n && n.offsetParent !== null; }
    function box(n) { const r = n?.getBoundingClientRect(); return r ? { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height), bottom: Math.round(r.bottom) } : null; }
    return {
      loginVisible: visible(login),
      appVisible: visible(document.querySelector('#app-view')),
      bodyScrollWidth: document.body.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
      bodyScrollHeight: document.body.scrollHeight,
      clientHeight: document.documentElement.clientHeight,
      card: box(document.querySelector('.login-card')),
      copy: box(document.querySelector('.login-copy')),
      subtitle: document.querySelector('#login-sub')?.textContent || '',
      help: document.querySelector('#login-help')?.innerText || '',
      adminVisible: visible(adminFields),
      userVisible: visible(userField),
      adminTabActive: document.querySelector('#seg-admin')?.classList.contains('active') || false,
      userTabActive: document.querySelector('#seg-user')?.classList.contains('active') || false,
      passLabel: passLabel?.textContent || '',
      userLabel: userLabel?.textContent || '',
      passPlaceholder: pass?.getAttribute('placeholder') || '',
      apiPlaceholder: api?.getAttribute('placeholder') || '',
      signInText: signIn?.innerText?.trim() || '',
      text: (login?.innerText || document.body.innerText || '').slice(0, 1400),
    };
  });
}

function checkCommon(label, metrics) {
  if (!metrics.loginVisible) fail(`${label}: login view missing`, metrics);
  if (metrics.appVisible) fail(`${label}: app visible before login`, metrics);
  if (metrics.bodyScrollWidth > metrics.clientWidth + 8) fail(`${label}: horizontal overflow`, metrics);
  if (!/Sign in/i.test(metrics.signInText)) fail(`${label}: missing sign-in button`, metrics);
}
function checkAdmin(label, metrics) {
  checkCommon(label, metrics);
  if (!metrics.adminTabActive || metrics.userTabActive) fail(`${label}: admin tab state incorrect`, metrics);
  if (!metrics.adminVisible || metrics.userVisible) fail(`${label}: admin form visibility incorrect`, metrics);
  if (!/master key/i.test(metrics.subtitle) || !/ADMIN_MASTER_KEY/i.test(metrics.help)) fail(`${label}: admin copy does not explain .env master key`, metrics);
  if (!/Admin key/i.test(metrics.passLabel) || metrics.passPlaceholder !== 'ADMIN_MASTER_KEY') fail(`${label}: admin credential label is unclear`, metrics);
}
function checkUser(label, metrics) {
  checkCommon(label, metrics);
  if (!metrics.userTabActive || metrics.adminTabActive) fail(`${label}: user tab state incorrect`, metrics);
  if (metrics.adminVisible || !metrics.userVisible) fail(`${label}: user form visibility incorrect`, metrics);
  if (!/client API key/i.test(metrics.subtitle) || !/lc-/i.test(metrics.help) || !/API Keys/i.test(metrics.help)) fail(`${label}: user copy does not explain client API key`, metrics);
  if (!/Client API key/i.test(metrics.userLabel) || metrics.apiPlaceholder !== 'lc-...') fail(`${label}: user API key label is unclear`, metrics);
}

async function runViewport(browser, name, width, height) {
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width, height } });
  await gotoWithRetry(page, BASE + '/');
  await page.locator('#login-view').waitFor({ state: 'visible', timeout: 12000 });
  const adminShot = `${OUT}/${name}-admin-login.png`;
  await page.screenshot({ path: adminShot, fullPage: true });
  result.screenshots.push(adminShot);
  const adminMetrics = await inspect(page);
  result.pages[`${name}-admin`] = adminMetrics;
  checkAdmin(`${name}-admin`, adminMetrics);

  await page.locator('#seg-user').click();
  await page.waitForTimeout(100);
  const userShot = `${OUT}/${name}-user-login.png`;
  await page.screenshot({ path: userShot, fullPage: true });
  result.screenshots.push(userShot);
  const userMetrics = await inspect(page);
  result.pages[`${name}-user`] = userMetrics;
  checkUser(`${name}-user`, userMetrics);
  await page.close();
}

async function main() {
  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  const browser = await chromium.launch(launch);
  try {
    await runViewport(browser, 'desktop-1440', 1440, 1000);
    await runViewport(browser, 'mobile-390', 390, 844);
  } finally {
    await browser.close().catch(() => {});
  }
}

await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(`${OUT}/summary.json`, JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
