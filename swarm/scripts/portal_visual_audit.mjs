import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const MODEL = `pw-visual-${Date.now().toString().slice(-8)}-super-long-public-model-name-for-enterprise-qwen3-flash-next-1m-token-router-route-overflow-check`;
const MOCK_URL = process.env.BRIGHTO_VISUAL_BACKEND_URL || 'http://127.0.0.1:9000/v1';

const result = { result: 'FAIL', base: BASE, model: MODEL, viewportResults: {}, failures: [], passes: [], screenshots: [] };
function pass(area, summary, evidence = {}) { console.log(`[PASS] ${area}: ${summary}`); result.passes.push({ area, summary, evidence }); }
function fail(area, summary, evidence = {}, requiredFix = '') { console.log(`[FAIL] ${area}: ${summary}`); result.failures.push({ area, summary, evidence, requiredFix }); }


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
  const res = await fetch(BASE + path, {
    method,
    headers: { 'content-type': 'application/json', 'x-admin-key': ADMIN },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status} ${text}`);
  return res.status === 204 || !text ? null : JSON.parse(text);
}

async function cleanup() {
  try { await adminFetch('/admin/routes/' + encodeURIComponent(MODEL), 'DELETE'); } catch {}
  try {
    const routes = await adminFetch('/admin/routes');
    for (const r of routes) {
      const name = String(r.model_name || '');
      if (name.startsWith('pw-visual-')) {
        try { await adminFetch('/admin/routes/' + encodeURIComponent(name), 'DELETE'); } catch {}
      }
    }
    const backends = await adminFetch('/admin/backends');
    for (const b of backends) {
      if (b.base_url === MOCK_URL && b.can_delete !== false) {
        try { await adminFetch('/admin/backends/' + b.id, 'DELETE'); } catch {}
      }
    }
  } catch {}
}

async function setupRoute() {
  const backends = await adminFetch('/admin/backends');
  let backend = backends.find((b) => b.base_url === MOCK_URL);
  if (!backend) {
    backend = await adminFetch('/admin/backends', 'POST', {
      name: 'Visual Audit Custom LLM',
      base_url: MOCK_URL,
      api_key_ref: 'env:NONE',
      weight: 1,
      max_inflight: 0,
      format: 'openai',
      enabled: true,
    });
  }
  await adminFetch('/admin/routes', 'POST', {
    model_name: MODEL,
    backend_ids: [backend.id],
    chars_per_token: 4,
    first_byte_timeout: 180,
    provider_model_name: 'mock-model',
    auth_mode: 'none',
    protocol: 'local_openai_chat',
    enabled: true,
    context_tokens: 1000000,
    max_output_tokens: 8192,
    price_input_per_mtok_usd: 0,
    price_output_per_mtok_usd: 0,
  });
}

async function login(page) {
  let lastErr;
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      await gotoWithRetry(page, BASE + '/');
      await page.locator('#login-user').fill('admin');
      await page.locator('#login-pass').fill(ADMIN);
      await page.evaluate(() => login());
      await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
      return;
    } catch (e) {
      lastErr = e;
      if (attempt < 3) await page.waitForTimeout(600);
    }
  }
  throw lastErr;
}

async function isActionableInViewport(page, locator) {
  if (!(await locator.isVisible().catch(() => false))) return false;
  const box = await locator.boundingBox().catch(() => null);
  if (!box) return false;
  const vp = page.viewportSize();
  return box.x >= 0 && box.y >= 0 && box.x + box.width <= vp.width && box.y + Math.min(box.height, 80) <= vp.height;
}

async function openModels(page, viewportName) {
  const nav = page.locator('.nav[data-view="models"]');
  if (await isActionableInViewport(page, nav)) {
    await nav.click();
    return true;
  }
  const hamburger = page.locator('.hamburger');
  if (await isActionableInViewport(page, hamburger)) {
    await hamburger.click();
    await page.waitForTimeout(350);
    if (await isActionableInViewport(page, nav)) {
      await nav.click();
      return true;
    }
  }
  fail('responsive', `${viewportName}: cannot reach Models via visible navigation`, {}, 'Mobile/tablet navigation must expose Models through the hamburger without force-clicking off-screen elements.');
  await page.evaluate(() => go('models'));
  return false;
}

function rectToObj(r) {
  if (!r) return null;
  return {
    x: r.x, y: r.y, width: r.width, height: r.height, right: r.right, bottom: r.bottom,
  };
}

async function inspectViewport(browser, name, width, height) {
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width, height } });
  const consoleErrors = [];
  page.on('console', (m) => { if (m.type() === 'error' && !/ERR_NETWORK_CHANGED|ERR_CONNECTION_RESET|ERR_HTTP2_PROTOCOL_ERROR/i.test(m.text())) consoleErrors.push(m.text()); });
  page.on('pageerror', (e) => consoleErrors.push('pageerror: ' + e.message));
  try {
    await login(page);
    const navOk = await openModels(page, name);
    await page.waitForTimeout(700);
    const shot = `${OUT}/${name}-models.png`;
    await page.screenshot({ path: shot, fullPage: true });
    result.screenshots.push(shot);

    const metrics = await page.evaluate((model) => {
      const row = [...document.querySelectorAll('tr')].find((tr) => tr.innerText.includes(model));
      const wrap = row ? row.closest('.table-wrap') : null;
      const cells = row ? [...row.querySelectorAll('td')] : [];
      const firstCell = cells[0] || null;
      const statusCell = cells[1] || null;
      const providerCell = cells[2] || null;
      const providerModelCell = cells[3] || null;
      const actions = row ? row.querySelector('td.actions') : null;
      const actionButtons = actions ? [...actions.querySelectorAll('button')].map((button) => {
        const r = button.getBoundingClientRect();
        return {
          text: (button.innerText || button.textContent || '').trim(),
          x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height),
          right: Math.round(r.right), bottom: Math.round(r.bottom),
        };
      }) : [];
      const modelText = document.body.innerText.includes(model);
      const sidebar = document.querySelector('.sidebar');
      const main = document.querySelector('.main');
      const content = document.querySelector('#content');
      const panel = document.querySelector('.panel');
      function detail(el) {
        if (!el) return null;
        const r = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        return {
          x: r.x, y: r.y, width: r.width, height: r.height, right: r.right, bottom: r.bottom,
          scrollWidth: el.scrollWidth, clientWidth: el.clientWidth,
          overflow: cs.overflow, textOverflow: cs.textOverflow, whiteSpace: cs.whiteSpace,
          text: el.innerText,
        };
      }
      return {
        viewport: { width: innerWidth, height: innerHeight },
        bodyScrollWidth: document.body.scrollWidth,
        htmlClientWidth: document.documentElement.clientWidth,
        modelText,
        hasTableRow: !!row,
        wrap: detail(wrap),
        row: detail(row),
        firstCell: detail(firstCell),
        statusCell: detail(statusCell),
        providerCell: detail(providerCell),
        providerModelCell: detail(providerModelCell),
        actions: detail(actions),
        actionButtons,
        sidebar: detail(sidebar),
        sidebarOpen: sidebar ? sidebar.classList.contains('open') : false,
        main: detail(main),
        content: detail(content),
        panel: detail(panel),
      };
    }, MODEL);
    metrics.navOk = navOk;
    metrics.consoleErrors = consoleErrors;
    result.viewportResults[name] = metrics;

    if (!metrics.modelText) {
      fail('visual', `${name}: long model name is not visible anywhere`, metrics, 'Models view must render the created route.');
      return;
    }
    if (consoleErrors.length) {
      fail('runtime', `${name}: browser console errors`, { consoleErrors }, 'No console errors during navigation/render.');
    }
    if (metrics.bodyScrollWidth > metrics.htmlClientWidth + 8) {
      fail('responsive', `${name}: whole page has horizontal overflow`, {
        bodyScrollWidth: metrics.bodyScrollWidth,
        clientWidth: metrics.htmlClientWidth,
      }, 'Only the table/card region may scroll horizontally; the whole app page must not.');
    } else {
      pass('responsive', `${name}: no whole-page horizontal overflow`);
    }

    if (width <= 820) {
      if (metrics.sidebarOpen) {
        fail('responsive', `${name}: sidebar remains open after menu navigation and covers content`, {
          sidebar: metrics.sidebar,
          main: metrics.main,
          panel: metrics.panel,
        }, 'After a mobile nav item is selected, close the sidebar overlay so the selected page is readable.');
      } else {
        pass('responsive', `${name}: sidebar closes after mobile navigation`, { sidebarOpen: metrics.sidebarOpen });
      }
      const visiblePanelWidth = metrics.panel ? Math.max(0, Math.min(metrics.panel.right, metrics.viewport.width) - Math.max(metrics.panel.x, 0)) : 0;
      if (!metrics.panel || visiblePanelWidth < Math.min(320, metrics.viewport.width - 32)) {
        fail('responsive', `${name}: selected page content is too narrow on mobile`, {
          visiblePanelWidth,
          panel: metrics.panel,
          viewport: metrics.viewport,
        }, 'Mobile content should occupy the viewport width with normal padding; sidebar must not reserve or cover most of the screen.');
      } else {
        pass('responsive', `${name}: selected page content width is usable on mobile`, { visiblePanelWidth });
      }
    }

    if (metrics.hasTableRow) {
      const c = metrics.firstCell;
      if (c && c.scrollWidth > c.clientWidth + 4 && !(c.overflow === 'hidden' && c.textOverflow === 'ellipsis')) {
        fail('visual', `${name}: long public model overflows without ellipsis`, c, 'Wrap model text in a truncating element/cell: overflow:hidden; text-overflow:ellipsis; white-space:nowrap; title=full name; copy action.');
      } else {
        pass('visual', `${name}: public model cell is controlled`, c);
      }
      for (const [label, cell] of [['provider', metrics.providerCell], ['provider model', metrics.providerModelCell]]) {
        if (cell && cell.scrollWidth > cell.clientWidth + 4 && !(cell.overflow === 'hidden' && cell.textOverflow === 'ellipsis')) {
          fail('visual', `${name}: ${label} cell overflows into the next column`, cell, 'Provider and provider-model cells must use the same truncation pattern as public model: overflow hidden, ellipsis, title/copy where useful.');
        } else {
          pass('visual', `${name}: ${label} cell is controlled`, cell);
        }
      }
      const a = metrics.actions;
      if (a && a.scrollWidth > a.clientWidth + 4) {
        fail('visual', `${name}: action column too narrow`, a, 'Reserve enough width for Disable/Edit/Delete or collapse to an Actions menu.');
      } else {
        pass('visual', `${name}: action column is wide enough`, a);
      }
      if (width >= 1200 && metrics.actionButtons && metrics.actionButtons.length >= 3) {
        const ys = metrics.actionButtons.map((b) => b.y);
        const sameRow = Math.max(...ys) - Math.min(...ys) <= 3;
        if (!sameRow) {
          fail('visual', `${name}: route action buttons stack vertically on desktop`, { actionButtons: metrics.actionButtons }, 'Desktop route rows must keep Disable/Edit/Delete on one compact row.');
        } else {
          pass('visual', `${name}: route action buttons stay on one desktop row`, { actionButtons: metrics.actionButtons });
        }
      }
    } else {
      pass('visual', `${name}: non-table route layout detected`, { modelText: true });
    }
  } catch (e) {
    fail('audit', `${name}: visual audit crashed`, { error: e.stack || e.message });
  } finally {
    await page.close().catch(() => {});
  }
}

async function main() {
  if (!ADMIN) { fail('environment', 'BRIGHTO_ADMIN_KEY is required'); return; }
  await cleanup();
  await setupRoute();
  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  const browser = await chromium.launch(launch);
  try {
    await inspectViewport(browser, 'desktop-1440', 1440, 1000);
    await inspectViewport(browser, 'laptop-1024', 1024, 800);
    await inspectViewport(browser, 'mobile-390', 390, 844);
  } finally {
    await browser.close().catch(() => {});
    await cleanup();
  }
}

await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(OUT + '/summary.json', JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
