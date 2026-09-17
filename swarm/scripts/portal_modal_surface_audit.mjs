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
      return { label, top: Math.round(r.top), bottom: Math.round(r.bottom), width: Math.round(r.width), visible: node.offsetParent !== null };
    });
    const visibleInputLabels = inputs.filter((i) => i.visible).map((i) => i.label);
    const wizardSteps = [...modal.querySelectorAll('.wizard-step')].map((node) => {
      const r = node.getBoundingClientRect();
      return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height), text: (node.innerText || '').trim().replace(/\s+/g, ' ') };
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
      visibleInputLabels,
      wizardSteps,
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
  if (name.startsWith('mobile-')) {
    if (metrics.rect && metrics.rect.bottom > metrics.clientH + 3) fail(`${name}: mobile modal extends below viewport instead of scrolling internally`, metrics);
    if (metrics.modalScrollHeight <= metrics.modalClientHeight && name.endsWith('add-model') && metrics.rect.height > metrics.clientH - 30) fail(`${name}: long mobile modal should scroll internally`, metrics);
  }
  if (name.endsWith('add-model')) {
    if (!/Exact upstream model name returned by the provider/i.test(metrics.text || '')) fail(`${name}: Add model modal missing Provider model help text`, metrics);
    if (!/model name your apps send/i.test(metrics.text || '')) fail(`${name}: Add model modal missing Public model help text`, metrics);
    if (!/Client sends|Provider receives/i.test(metrics.text || '')) fail(`${name}: Add model modal missing public-to-provider model mapping preview`, metrics);
    if (!/Gemini \(coming soon\)|Meta Muse \(coming soon\)/i.test(metrics.text || '')) fail(`${name}: Add model provider picker must mark coming-soon providers`, metrics);
    if (name.startsWith('desktop-') && metrics.footer && metrics.footer.bottom > metrics.clientH + 3) fail(`${name}: Add model primary actions must be visible on desktop`, metrics);
    if (!/Optional limits and pricing|fallback provider/i.test(metrics.text || '')) fail(`${name}: Add model modal missing optional limits/pricing drawer`, metrics);
    if (/Fallback backend/i.test(metrics.text || '')) fail(`${name}: Add model modal exposes backend jargon`, metrics);
    if (!/Save disabled/i.test(metrics.text || '')) fail(`${name}: Add model modal must make disabled save explicit`, metrics);
    if (/Save draft/i.test(metrics.text || '')) fail(`${name}: Add model modal exposes ambiguous Save draft action`, metrics);
    if (name.startsWith('mobile-')) {
      const steps = metrics.wizardSteps || [];
      const firstRow = steps.filter((r) => steps[0] && Math.abs(r.y - steps[0].y) <= 4);
      if (steps.length !== 3 || firstRow.length !== 3) fail(`${name}: Add model wizard steps must stay compact on one mobile row`, metrics);
      const cramped = steps.filter((r) => r.width < 90 || r.height > 72);
      if (cramped.length) fail(`${name}: Add model wizard step chips are cramped on mobile`, { cramped, metrics });
    }
  }
  if (name.endsWith('new-key')) {
    if (!/Model access|All models|Restrict to selected models/i.test(metrics.text || '')) fail(`${name}: New key modal missing guided model access picker`, metrics);
    if (!/Optional limits and budget/i.test(metrics.text || '')) fail(`${name}: New key modal missing collapsed optional limits/budget drawer`, metrics);
    if (/comma-separated|empty = all/i.test(metrics.text || '')) fail(`${name}: New key modal still exposes comma-separated model entry`, metrics);
    const visibleLabels = (metrics.visibleInputLabels || []).map((x) => String(x).toLowerCase());
    const optionalLabels = ['expiry date (optional)', 'requests per minute (optional)', 'simultaneous request limit (optional)', 'budget', 'period', 'token amount'];
    const hiddenByDefault = optionalLabels.filter((label) => visibleLabels.includes(label));
    if (hiddenByDefault.length) fail(`${name}: New key optional limit fields must be collapsed by default`, { hiddenByDefault, metrics });
    await page.getByRole('button', { name: /Optional limits and budget/ }).click({ force: true });
    await page.waitForTimeout(200);
    const expanded = await inspectModal(page);
    result.metrics[`${name}-expanded`] = expanded;
    const expandedLabels = (expanded.visibleInputLabels || []).map((x) => String(x).toLowerCase());
    const missingExpanded = optionalLabels.filter((label) => !expandedLabels.includes(label));
    if (missingExpanded.length) fail(`${name}: New key optional drawer did not reveal all limit/budget fields`, { missingExpanded, expanded });
  }
  if (name.endsWith('new-team')) {
    if (!/Optional team budget/i.test(metrics.text || '')) fail(`${name}: New team modal missing optional team budget drawer`, metrics);
    if (/Advanced JSON/i.test(metrics.text || '')) fail(`${name}: budget advanced action exposes JSON jargon`, metrics);
    const visibleLabels = (metrics.visibleInputLabels || []).map((x) => String(x).toLowerCase());
    const optionalLabels = ['budget type', 'period', 'token amount'];
    const hiddenByDefault = optionalLabels.filter((label) => visibleLabels.includes(label));
    if (hiddenByDefault.length) fail(`${name}: New team budget fields must be collapsed by default`, { hiddenByDefault, metrics });
    await page.getByRole('button', { name: /Optional team budget/ }).click({ force: true });
    await page.waitForTimeout(200);
    const expanded = await inspectModal(page);
    result.metrics[`${name}-expanded`] = expanded;
    const expandedLabels = (expanded.visibleInputLabels || []).map((x) => String(x).toLowerCase());
    const missingExpanded = optionalLabels.filter((label) => !expandedLabels.includes(label));
    if (missingExpanded.length || !/Advanced budget rules/i.test(expanded.text || '')) fail(`${name}: New team optional budget drawer did not reveal all budget controls`, { missingExpanded, expanded });
  }
  if (name.endsWith('new-key')) {
    if (/Advanced JSON/i.test(metrics.text || '')) fail(`${name}: budget advanced action exposes JSON jargon`, metrics);
  }
  if (name.endsWith('add-provider')) {
    if (!/Optional load control/i.test(metrics.text || '')) fail(`${name}: Provider modal missing optional load control drawer`, metrics);
    const visibleLabels = (metrics.visibleInputLabels || []).map((x) => String(x).toLowerCase());
    const optionalLabels = ['weight', 'simultaneous calls (0 = unlimited)'];
    const hiddenByDefault = optionalLabels.filter((label) => visibleLabels.includes(label));
    if (hiddenByDefault.length) fail(`${name}: Provider load-control fields must be collapsed by default`, { hiddenByDefault, metrics });
    await page.getByRole('button', { name: /Optional load control/ }).click({ force: true });
    await page.waitForTimeout(200);
    const expanded = await inspectModal(page);
    result.metrics[`${name}-expanded`] = expanded;
    const expandedLabels = (expanded.visibleInputLabels || []).map((x) => String(x).toLowerCase());
    const missingExpanded = optionalLabels.filter((label) => !expandedLabels.includes(label));
    if (missingExpanded.length) fail(`${name}: Provider optional load-control drawer did not reveal all controls`, { missingExpanded, expanded });
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
