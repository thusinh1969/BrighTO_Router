import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const stamp = Date.now().toString().slice(-7);

const result = {
  result: 'FAIL',
  failures: [],
  bugs: [],
  evidence: {},
  consoleErrors: [],
};

function bug(message) { result.bugs.push(message); }
function fail(message) { result.failures.push(message); }

async function login(page) {
  await page.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 20000 });
  await page.locator('#login-user').fill('admin');
  await page.locator('#login-pass').fill(ADMIN);
  await page.evaluate(() => login());
  await page.getByRole('heading', { name: 'Dashboard' }).waitFor({ state: 'visible', timeout: 10000 });
}

async function nav(page, name) {
  await page.getByRole('button', { name }).first().click({ force: true });
  await page.waitForTimeout(350);
}

async function field(page, label) {
  return page
    .locator('.modal .field')
    .filter({ has: page.locator('label', { hasText: label }) })
    .first()
    .locator('input,textarea,select')
    .first();
}

async function fill(page, label, value) {
  await (await field(page, label)).fill(String(value));
}

async function setMaybeSelect(page, label, value) {
  const loc = await field(page, label);
  const tag = await loc.evaluate((e) => e.tagName.toLowerCase()).catch(() => null);
  if (tag === 'select') await loc.selectOption(String(value));
  else await loc.fill(String(value));
}

async function row(page, text) {
  return page.locator('tr').filter({ hasText: text }).first();
}

async function panels(page, text) {
  return page.locator('.panel').filter({ hasText: text }).count();
}

async function modalButton(page, name) {
  return page.locator('.modal').getByRole('button', { name }).first();
}

async function clickRowButton(page, rowText, buttonName) {
  const r = await row(page, rowText);
  await r.waitFor({ state: 'visible', timeout: 7000 });
  await r.getByRole('button', { name: buttonName }).first().click({ force: true });
}

async function main() {
  if (!ADMIN) fail('BRIGHTO_ADMIN_KEY is required');
  if (result.failures.length) return;

  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  const browser = await chromium.launch(launch);
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, ignoreHTTPSErrors: true });
  page.on('console', (m) => { if (m.type() === 'error') result.consoleErrors.push(m.text()); });
  page.on('pageerror', (e) => result.consoleErrors.push('pageerror:' + e.message));
  page.on('dialog', async (d) => { result.evidence.lastDialog = d.message(); await d.accept(); });

  try {
    await login(page);
    result.evidence.loginOk = true;

    result.evidence.formatters = await page.evaluate(() => ({
      fmt1000: typeof fmt === 'function' ? fmt(1000) : null,
      fmt50000: typeof fmt === 'function' ? fmt(50000) : null,
      fmt1000000: typeof fmt === 'function' ? fmt(1000000) : null,
      fmtNum1000: typeof fmtNum === 'function' ? fmtNum(1000) : null,
      fmtNum1000000: typeof fmtNum === 'function' ? fmtNum(1000000) : null,
    }));
    if (result.evidence.formatters.fmt1000 !== '1K') {
      bug(`Main formatter must use K/M/B: fmt(1000)=${result.evidence.formatters.fmt1000}`);
    }
    if (String(result.evidence.formatters.fmtNum1000).includes('k')) {
      bug(`Chart formatter must use uppercase K: fmtNum(1000)=${result.evidence.formatters.fmtNum1000}`);
    }

    result.evidence.dashboardDensity = await page.evaluate(() => ({
      bodyFont: getComputedStyle(document.body).fontSize,
      titleFont: getComputedStyle(document.querySelector('.topbar h2')).fontSize,
      cardBig: getComputedStyle(document.querySelector('.card .big')).fontSize,
      panelPadding: getComputedStyle(document.querySelector('.panel')).padding,
      panelMargin: getComputedStyle(document.querySelector('.panel')).marginBottom,
      cardHeight: Math.round(document.querySelector('.card').getBoundingClientRect().height),
    }));
    if (parseFloat(result.evidence.dashboardDensity.cardBig) >= 30) {
      bug(`Dashboard big-number font too large for compact admin view: ${result.evidence.dashboardDensity.cardBig}`);
    }
    if (parseFloat(result.evidence.dashboardDensity.panelPadding) >= 20) {
      bug(`Panel padding too airy for compact admin view: ${result.evidence.dashboardDensity.panelPadding}`);
    }

    await nav(page, 'Settings');
    const settingsText = await page.locator('#content').innerText();
    result.evidence.settingsText = settingsText;
    if (!/Font size|Small|Large/i.test(settingsText)) bug('Settings missing Font size: Small / Normal / Large.');
    if (!/Density|Compact|Comfortable/i.test(settingsText)) bug('Settings missing Density: Compact / Comfortable.');
    result.evidence.rootPrefs = await page.evaluate(() => ({
      htmlFont: document.documentElement.getAttribute('data-font'),
      htmlDensity: document.documentElement.getAttribute('data-density'),
      localStorageKeys: Object.keys(localStorage).filter((k) => /pref|font|density|brighto/i.test(k)),
    }));

    await nav(page, 'Providers');
    const providerName = `pw-polish-provider-${stamp}`;
    const providerEdit = `${providerName}-edited`;
    await page.getByRole('button', { name: 'Add provider' }).first().click({ force: true });
    await fill(page, 'Name', providerName);
    await fill(page, 'Base URL', 'http://127.0.0.1:65531/v1');
    await setMaybeSelect(page, 'Format', 'openai');
    await fill(page, 'Weight', '1');
    await fill(page, 'Max concurrent', '0');
    await (await modalButton(page, 'Add')).click({ force: true });
    await (await row(page, providerName)).waitFor({ state: 'visible', timeout: 8000 });
    let providerPanels = await panels(page, 'Provider backends');
    result.evidence.providerPanelsAfterCreate = providerPanels;
    if (providerPanels !== 1) bug(`Provider create leaves ${providerPanels} Provider panels; expected exactly 1.`);

    await nav(page, 'Providers');
    await clickRowButton(page, providerName, 'Edit');
    await fill(page, 'Name', providerEdit);
    await (await modalButton(page, 'Save')).click({ force: true });
    await page.waitForTimeout(800);
    providerPanels = await panels(page, 'Provider backends');
    result.evidence.providerPanelsAfterEdit = providerPanels;
    if (providerPanels !== 1) bug(`Provider edit leaves ${providerPanels} Provider panels; expected exactly 1.`);

    await nav(page, 'Providers');
    await clickRowButton(page, providerEdit, 'Delete');
    await page.waitForTimeout(1000);
    providerPanels = await panels(page, 'Provider backends');
    result.evidence.providerPanelsAfterDelete = providerPanels;
    if (providerPanels !== 1) bug(`Provider delete leaves ${providerPanels} Provider panels; expected exactly 1.`);

    await nav(page, 'Usage');
    const usageText = await page.locator('#content').innerText();
    result.evidence.usageText = usageText.slice(0, 2500);
    if (!/Tok\/s|tokens per second/i.test(usageText)) bug('Usage/logs must visibly prioritize tok/s.');
    if (/\b\d{1,3},\d{3}\b/.test(usageText)) bug('Usage still shows comma-formatted large counts; expected compact K/M/B display.');
  } catch (e) {
    fail(e.stack || e.message);
  } finally {
    await browser.close();
  }
}

await main();
result.result = (result.failures.length || result.bugs.length) ? 'FAIL' : 'PASS';
await writeFile(`${OUT}/summary.json`, JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
