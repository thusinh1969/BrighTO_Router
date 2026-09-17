import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const result = { result: 'FAIL', base: BASE, failures: [], screenshots: [], metrics: {}, consoleErrors: [] };


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

function fail(summary, evidence = {}) { result.failures.push({ summary, evidence }); }

async function login(page) {
  await gotoWithRetry(page, BASE + '/');
  await page.locator('#login-user').fill('admin');
  await page.locator('#login-pass').fill(ADMIN);
  await page.evaluate(() => login());
  await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
}

async function nav(page, view) {
  await page.evaluate((v) => go(v), view);
  await page.waitForTimeout(500);
}

async function resetModal(page) {
  await page.evaluate(() => {
    const overlay = document.querySelector('#modal-overlay');
    if (overlay) {
      overlay.classList.add('hidden');
      overlay.innerHTML = '';
    }
    document.querySelectorAll('.picker-overlay').forEach((node) => node.remove());
  });
  await page.waitForTimeout(100);
}

async function inspectModal(page) {
  return page.evaluate(() => {
    const modal = document.querySelector('.modal');
    const overlay = document.querySelector('#modal-overlay:not(.hidden), .picker-overlay');
    const clientW = document.documentElement.clientWidth;
    const clientH = document.documentElement.clientHeight;
    if (!modal) return { missing: true, clientW, clientH };
    const rect = modal.getBoundingClientRect();
    const clipped = [];
    for (const node of modal.querySelectorAll('*')) {
      const r = node.getBoundingClientRect();
      const style = getComputedStyle(node);
      if (r.right > Math.min(clientW, rect.right) + 3 || node.scrollWidth > node.clientWidth + 5) {
        clipped.push({
          tag: node.tagName,
          cls: String(node.className),
          text: (node.innerText || node.textContent || '').slice(0, 140).replace(/\s+/g, ' '),
          right: Math.round(r.right),
          clientW,
          scrollWidth: node.scrollWidth,
          clientWidth: node.clientWidth,
          overflowX: style.overflowX,
        });
      }
    }
    const footer = modal.querySelector('.actions');
    const footerRect = footer ? footer.getBoundingClientRect() : null;
    const switches = [...modal.querySelectorAll('.switch-field')].map((node) => {
      const r = node.getBoundingClientRect();
      const style = getComputedStyle(node);
      return {
        text: (node.innerText || node.textContent || '').trim().replace(/\s+/g, ' '),
        display: style.display,
        justifyContent: style.justifyContent,
        textTransform: style.textTransform,
        width: Math.round(r.width),
        height: Math.round(r.height),
      };
    });
    const inputs = [...modal.querySelectorAll('input,select,textarea')].map((node) => {
      const r = node.getBoundingClientRect();
      const label = node.closest('.field')?.querySelector('label')?.innerText?.trim() || node.placeholder || node.tagName;
      return { label, top: Math.round(r.top), bottom: Math.round(r.bottom), width: Math.round(r.width) };
    });
    const footerCoveredInputs = footerRect
      ? inputs.filter((i) => i.bottom > footerRect.top + 4 && i.top < footerRect.bottom - 4)
      : [];
    return {
      missing: false,
      rect: { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height), bottom: Math.round(rect.bottom) },
      clientW,
      clientH,
      modalScrollHeight: modal.scrollHeight,
      modalClientHeight: modal.clientHeight,
      overlayScrollHeight: overlay ? overlay.scrollHeight : null,
      footer: footerRect ? { top: Math.round(footerRect.top), bottom: Math.round(footerRect.bottom), height: Math.round(footerRect.height) } : null,
      footerCoveredInputs,
      switches,
      text: modal.innerText.slice(0, 2400),
      clipped: clipped.slice(0, 25),
    };
  });
}

async function capture(page, name) {
  const shot = `${OUT}/${name}.png`;
  await page.screenshot({ path: shot, fullPage: true });
  result.screenshots.push(shot);
  const metrics = await inspectModal(page);
  result.metrics[name] = metrics;
  if (metrics.missing) fail(`${name}: modal missing`, metrics);
  if (metrics.clipped?.length) fail(`${name}: modal has clipped/overflowing content`, metrics);
  if (metrics.footerCoveredInputs?.length) fail(`${name}: sticky footer covers input fields`, metrics);
  if (name.endsWith('add-model')) {
    if (!/Exact upstream model name returned by the provider/i.test(metrics.text || '')) fail(`${name}: Add model modal missing Provider model help text`, metrics);
    if (!/model name your apps send/i.test(metrics.text || '')) fail(`${name}: Add model modal missing Public model help text`, metrics);
    if (!/Fallback provider \(optional\)/i.test(metrics.text || '')) fail(`${name}: Add model modal must expose fallback as provider selection`, metrics);
    if (/Fallback backend/i.test(metrics.text || '')) fail(`${name}: Add model modal exposes backend jargon`, metrics);
    if (!/Save disabled/i.test(metrics.text || '')) fail(`${name}: Add model modal must make disabled save explicit`, metrics);
    if (/Save draft/i.test(metrics.text || '')) fail(`${name}: Add model modal exposes ambiguous Save draft action`, metrics);
  }
  if (name.endsWith('add-provider') || name.endsWith('new-team')) {
    const badSwitches = (metrics.switches || []).filter((s) => s.display !== 'flex' || s.justifyContent !== 'space-between' || s.textTransform !== 'none');
    if (!(metrics.switches || []).length) fail(`${name}: state switch missing`, metrics);
    if (badSwitches.length) fail(`${name}: state switch layout regressed`, { badSwitches, metrics });
  }
}

async function runViewport(browser, name, width, height) {
  const page = await browser.newPage({ viewport: { width, height }, ignoreHTTPSErrors: true });
  page.on('console', (m) => { if (m.type() === 'error' && !/ERR_NETWORK_CHANGED/i.test(m.text())) result.consoleErrors.push(`${name}: ${m.text()}`); });
  page.on('pageerror', (e) => result.consoleErrors.push(`${name}: pageerror ${e.message}`));
  try {
    await login(page);
    for (const item of [
      ['add-model', 'models', () => page.evaluate(() => openRouteModal())],
      ['add-provider', 'providers', () => page.evaluate(() => openProviderModal())],
      ['new-team', 'teams', () => page.evaluate(() => openTeamModal())],
      ['new-key', 'keys', () => page.evaluate(() => openKeyModal())],
    ]) {
      const [suffix, view, open] = item;
      await nav(page, view);
      await open();
      await page.locator('.modal').waitFor({ state: 'visible', timeout: 7000 });
      await capture(page, `${name}-${suffix}`);
      await resetModal(page);
    }
  } finally {
    await page.close().catch(() => {});
  }
}

async function main() {
  if (!ADMIN) { fail('BRIGHTO_ADMIN_KEY is required'); return; }
  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  const browser = await chromium.launch(launch);
  try {
    await runViewport(browser, 'desktop', 1440, 1000);
    await runViewport(browser, 'mobile', 390, 844);
  } finally {
    await browser.close().catch(() => {});
  }
  if (result.consoleErrors.length) fail('console errors during modal audit', { consoleErrors: result.consoleErrors });
}

await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(`${OUT}/summary.json`, JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
