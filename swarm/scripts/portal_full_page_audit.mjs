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
async function verifyAdminReload(page, name) {
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
  const state = await page.evaluate(() => ({
    loginHidden: document.querySelector('#login-view')?.classList.contains('hidden') || false,
    appVisible: !(document.querySelector('#app-view')?.classList.contains('hidden') || false),
    modePill: document.querySelector('#mode-pill')?.textContent || '',
    visibleNav: [...document.querySelectorAll('.nav')].filter((n) => getComputedStyle(n).display !== 'none').map((n) => (n.innerText || n.textContent || '').trim()),
    title: document.querySelector('#page-title')?.textContent || '',
  }));
  if (!state.loginHidden || !state.appVisible || !/Admin/i.test(state.modePill) || !state.visibleNav.includes('Providers') || !state.visibleNav.includes('API Keys')) fail(`${name}: F5 reload should keep verified admin session`, state);
}
async function go(page, view, mobile) {
  if (mobile) {
    await page.locator('.hamburger').click({ force: true });
    await page.waitForTimeout(250);
  }
  await page.locator(`.nav[data-view="${view}"]`).click({ force: true });
  await page.waitForTimeout(700);
}
async function verifyMobileSidebar(page, name) {
  const initial = await page.evaluate(() => ({
    sidebarOpen: document.querySelector('#sidebar')?.classList.contains('open') || false,
    backdropActive: document.querySelector('#sidebar-backdrop')?.classList.contains('active') || false,
  }));
  if (initial.sidebarOpen || initial.backdropActive) fail(`${name}: mobile sidebar should start closed`, initial);

  await page.locator('.hamburger').click({ force: true });
  await page.waitForTimeout(250);
  const opened = await page.evaluate(() => {
    const sb = document.querySelector('#sidebar');
    const bd = document.querySelector('#sidebar-backdrop');
    return {
      sidebarOpen: sb?.classList.contains('open') || false,
      backdropActive: bd?.classList.contains('active') || false,
      bodyLocked: document.body.classList.contains('nav-open'),
      backdropPointer: bd ? getComputedStyle(bd).pointerEvents : '',
      backdropRect: bd ? (() => { const r = bd.getBoundingClientRect(); return { x: Math.round(r.x), width: Math.round(r.width) }; })() : null,
    };
  });
  if (!opened.sidebarOpen || !opened.backdropActive || !opened.bodyLocked || opened.backdropPointer === 'none' || !opened.backdropRect || opened.backdropRect.width < 80) fail(`${name}: hamburger should open sidebar with active backdrop`, opened);

  const vp = page.viewportSize() || { width: 390, height: 760 };
  await page.mouse.click(vp.width - 12, Math.floor(vp.height / 2));
  await page.waitForTimeout(250);
  const closedByBackdrop = await page.evaluate(() => ({
    sidebarOpen: document.querySelector('#sidebar')?.classList.contains('open') || false,
    backdropActive: document.querySelector('#sidebar-backdrop')?.classList.contains('active') || false,
    bodyLocked: document.body.classList.contains('nav-open'),
  }));
  if (closedByBackdrop.sidebarOpen || closedByBackdrop.backdropActive || closedByBackdrop.bodyLocked) fail(`${name}: tapping backdrop should close mobile sidebar`, closedByBackdrop);

  await page.locator('.hamburger').click({ force: true });
  await page.waitForTimeout(150);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(150);
  const closedByEscape = await page.evaluate(() => ({
    sidebarOpen: document.querySelector('#sidebar')?.classList.contains('open') || false,
    backdropActive: document.querySelector('#sidebar-backdrop')?.classList.contains('active') || false,
    bodyLocked: document.body.classList.contains('nav-open'),
  }));
  if (closedByEscape.sidebarOpen || closedByEscape.backdropActive || closedByEscape.bodyLocked) fail(`${name}: Escape should close mobile sidebar`, closedByEscape);
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
      const sampleCell = cells.find((td) => (td.getAttribute('data-label') || '').trim()) || cells[0];
      const sampleStyle = sampleCell ? getComputedStyle(sampleCell) : null;
      const beforeStyle = sampleCell ? getComputedStyle(sampleCell, '::before') : null;
      return {
        rows: rows.length,
        cells: cells.length,
        labelledCells,
        wrapWidth: Math.round(rect.width),
        scrollWidth: wrap.scrollWidth,
        clientWidth: wrap.clientWidth,
        tableDisplay: table ? getComputedStyle(table).display : '',
        bodyDisplay: table ? getComputedStyle(table.querySelector('tbody') || table).display : '',
        sampleCellDisplay: sampleStyle ? sampleStyle.display : '',
        sampleCellTextAlign: sampleStyle ? sampleStyle.textAlign : '',
        sampleBeforeDisplay: beforeStyle ? beforeStyle.display : '',
      };
    });
    const legendContainerStats = [...document.querySelectorAll('#content .legend')].map((node) => {
      const r = node.getBoundingClientRect();
      const st = getComputedStyle(node);
      return {
        display: st.display,
        flexWrap: st.flexWrap,
        overflowX: st.overflowX,
        overflowY: st.overflowY,
        width: Math.round(r.width),
        height: Math.round(r.height),
        scrollWidth: node.scrollWidth,
        clientWidth: node.clientWidth,
        itemCount: node.querySelectorAll('span').length,
      };
    });
    const legendStats = [...document.querySelectorAll('#content .legend span')].map((node) => ({
      text: (node.innerText || node.textContent || '').trim(),
      scrollWidth: node.scrollWidth,
      clientWidth: node.clientWidth,
      scrollHeight: node.scrollHeight,
      clientHeight: node.clientHeight,
    }));
    const chartValueLabels = [...document.querySelectorAll('#content .chart-value-label')].map((node) => {
      const r = node.getBoundingClientRect();
      return { text: (node.textContent || '').trim(), x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) };
    });
    const chartDataBars = [...document.querySelectorAll('#content .chart-data-bar')].map((node) => {
      const r = node.getBoundingClientRect();
      return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) };
    });
    const sectionPositions = [...document.querySelectorAll('#content h3, #content .diagnostic-details summary b')].map((node) => {
      const r = node.getBoundingClientRect();
      return { text: (node.innerText || node.textContent || '').trim(), y: Math.round(r.y) };
    });
    const diagnosticDetails = [...document.querySelectorAll('#content .diagnostic-details')].map((node) => {
      const r = node.getBoundingClientRect();
      return { open: node.open, text: (node.innerText || node.textContent || '').trim().slice(0, 240), y: Math.round(r.y), height: Math.round(r.height) };
    });
    const opsStatus = [...document.querySelectorAll('#content .ops-status')].map((node) => ({
      label: (node.querySelector('.k')?.textContent || '').trim(),
      value: (node.querySelector('.v')?.textContent || '').trim(),
      note: (node.querySelector('.cell-sub')?.textContent || '').trim(),
    }));
    const usageBreakdownStats = [...document.querySelectorAll('#content .usage-breakdown-grid')].map((grid) => {
      const st = getComputedStyle(grid);
      return {
        display: st.display,
        columns: st.gridTemplateColumns,
        cards: grid.querySelectorAll('.breakdown-card').length,
        tables: grid.querySelectorAll('table').length,
        clippedTitles: [...grid.querySelectorAll('.breakdown-title')].filter((n) => n.scrollHeight > n.clientHeight + 4 || n.scrollWidth > n.clientWidth + 4).map((n) => ({
          text: (n.textContent || '').trim(),
          scrollHeight: n.scrollHeight,
          clientHeight: n.clientHeight,
          scrollWidth: n.scrollWidth,
          clientWidth: n.clientWidth,
        })),
      };
    });
    const jsTruncatedLabels = [...document.querySelectorAll('#content .focus-title, #content .legend-label, #content .breakdown-title, #content .request-model, #content .model-name-row .cell-main, #content .compact-line')]
      .filter((n) => (n.textContent || '').includes('…'))
      .map((n) => ({ className: n.className || '', text: (n.textContent || '').trim(), title: n.title || '' }));
    const primaryDataLabels = [...document.querySelectorAll('#content .legend-label, #content .focus-title, #content .request-model')].map((n) => {
      const st = getComputedStyle(n);
      return {
        className: n.className || '',
        text: (n.innerText || n.textContent || '').trim(),
        whiteSpace: st.whiteSpace,
        overflow: st.overflow,
        textOverflow: st.textOverflow,
        wordBreak: st.wordBreak,
        overflowWrap: st.overflowWrap,
        scrollWidth: n.scrollWidth,
        clientWidth: n.clientWidth,
        scrollHeight: n.scrollHeight,
        clientHeight: n.clientHeight,
      };
    });
    const requestStatusPills = [...document.querySelectorAll('#content .request-card-head > .pill')].map((n) => {
      const r = n.getBoundingClientRect();
      const st = getComputedStyle(n);
      return {
        text: (n.innerText || n.textContent || '').trim(),
        width: Math.round(r.width),
        height: Math.round(r.height),
        justifySelf: st.justifySelf,
        alignSelf: st.alignSelf,
        whiteSpace: st.whiteSpace,
      };
    });
    const requestKpis = [...document.querySelectorAll('#content .request-kpi')].map((n) => {
      const label = (n.querySelector('.k')?.textContent || '').trim();
      const value = n.querySelector('.v');
      const st = value ? getComputedStyle(value) : null;
      return {
        label,
        value: value ? (value.innerText || value.textContent || '').trim() : '',
        scrollWidth: value ? value.scrollWidth : 0,
        clientWidth: value ? value.clientWidth : 0,
        scrollHeight: value ? value.scrollHeight : 0,
        clientHeight: value ? value.clientHeight : 0,
        whiteSpace: st ? st.whiteSpace : '',
        overflow: st ? st.overflow : '',
        textOverflow: st ? st.textOverflow : '',
      };
    });
    return {
      title: document.querySelector('#page-title')?.textContent || '',
      panelCount,
      cards,
      summaryCards,
      bodyScrollWidth: document.body.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
      contentText: text.slice(0, 1200),
      visibleToasts: [...document.querySelectorAll('#toast .toast')].map((n) => (n.innerText || n.textContent || '').trim()),
      tableStats,
      legendContainerStats,
      legendStats,
      chartValueLabels,
      chartDataBars,
      sectionPositions,
      diagnosticDetails,
      opsStatus,
      usageBreakdownStats,
      jsTruncatedLabels,
      primaryDataLabels,
      requestStatusPills,
      requestKpis,
      disabledDangerButtons: [...document.querySelectorAll('button.btn.danger:disabled')].filter((b) => b.offsetParent !== null).map((b) => {
        const st = getComputedStyle(b);
        return { text: b.innerText.trim(), title: b.title || '', color: st.color, borderColor: st.borderColor, opacity: st.opacity };
      }),
      modelNameRows: [...document.querySelectorAll('.models-table .model-name-row')].map((n) => {
        const st = getComputedStyle(n);
        const r = n.getBoundingClientRect();
        const name = n.querySelector('.cell-main');
        const ns = name ? getComputedStyle(name) : null;
        return {
          display: st.display,
          columns: st.gridTemplateColumns,
          width: Math.round(r.width),
          copyButtons: n.querySelectorAll('button.icon-btn').length,
          nameText: name ? (name.innerText || name.textContent || '').trim() : '',
          nameScrollHeight: name ? name.scrollHeight : 0,
          nameClientHeight: name ? name.clientHeight : 0,
          nameTextOverflow: ns ? ns.textOverflow : '',
          nameLineClamp: ns ? ns.webkitLineClamp : '',
          nameOverflow: ns ? ns.overflow : '',
        };
      }),
      routeListStats: [...document.querySelectorAll('.route-list-table')].map((table) => {
        const tbody = table.querySelector('tbody');
        const thead = table.querySelector('thead');
        const firstRow = table.querySelector('tbody tr');
        const firstAction = table.querySelector('td.actions .action-row');
        const firstNameCell = table.querySelector('tbody tr td:nth-child(1)');
        const wrap = table.closest('.route-list-wrap');
        const nr = firstNameCell ? firstNameCell.getBoundingClientRect() : null;
        const wr = wrap ? wrap.getBoundingClientRect() : null;
        const actionButtons = firstAction ? [...firstAction.querySelectorAll('button')].map((b) => {
          const r = b.getBoundingClientRect();
          return { text: b.innerText.trim(), x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height), right: Math.round(r.right), visible: r.width > 0 && r.height > 0 };
        }) : [];
        const metricCells = firstRow ? [...firstRow.querySelectorAll('td:nth-child(2),td:nth-child(3),td:nth-child(4),td:nth-child(5)')].map((cell) => {
          const st = getComputedStyle(cell);
          const r = cell.getBoundingClientRect();
          return { text: (cell.innerText || cell.textContent || '').trim(), width: Math.round(r.width), height: Math.round(r.height), borderStyle: st.borderStyle, borderRadius: st.borderRadius, backgroundColor: st.backgroundColor };
        }) : [];
        return {
          tableDisplay: getComputedStyle(table).display,
          bodyDisplay: tbody ? getComputedStyle(tbody).display : '',
          headDisplay: thead ? getComputedStyle(thead).display : '',
          rowDisplay: firstRow ? getComputedStyle(firstRow).display : '',
          rowColumns: firstRow ? getComputedStyle(firstRow).gridTemplateColumns : '',
          actionDisplay: firstAction ? getComputedStyle(firstAction).display : '',
          publicNameCellWidth: nr ? Math.round(nr.width) : 0,
          wrapRight: wr ? Math.round(wr.right) : 0,
          actionButtons,
          metricCells,
          wrapBorder: wrap ? getComputedStyle(wrap).borderStyle : '',
          rows: table.querySelectorAll('tbody tr').length,
        };
      }),
      providerListStats: [...document.querySelectorAll('.provider-list-table')].map((table) => {
        const tbody = table.querySelector('tbody');
        const thead = table.querySelector('thead');
        const firstRow = table.querySelector('tbody tr');
        const firstAction = table.querySelector('td.actions .action-row');
        const wrap = table.closest('.provider-list-wrap');
        const metricCells = firstRow ? [...firstRow.querySelectorAll('td:nth-child(3),td:nth-child(4),td:nth-child(5)')].map((cell) => {
          const st = getComputedStyle(cell);
          const r = cell.getBoundingClientRect();
          return { text: (cell.innerText || cell.textContent || '').trim(), width: Math.round(r.width), height: Math.round(r.height), borderStyle: st.borderStyle, borderRadius: st.borderRadius, backgroundColor: st.backgroundColor };
        }) : [];
        return {
          tableDisplay: getComputedStyle(table).display,
          bodyDisplay: tbody ? getComputedStyle(tbody).display : '',
          headDisplay: thead ? getComputedStyle(thead).display : '',
          rowDisplay: firstRow ? getComputedStyle(firstRow).display : '',
          rowColumns: firstRow ? getComputedStyle(firstRow).gridTemplateColumns : '',
          actionDisplay: firstAction ? getComputedStyle(firstAction).display : '',
          actionButtons: firstAction ? [...firstAction.querySelectorAll('button')].map((button) => { const r = button.getBoundingClientRect(); return { text: (button.innerText || button.textContent || '').trim(), x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) }; }) : [],
          metricCells,
          wrapBorder: wrap ? getComputedStyle(wrap).borderStyle : '',
          rows: table.querySelectorAll('tbody tr').length,
        };
      }),
      teamListStats: [...document.querySelectorAll('.team-list-table')].map((table) => {
        const tbody = table.querySelector('tbody');
        const thead = table.querySelector('thead');
        const firstRow = table.querySelector('tbody tr');
        const firstAction = table.querySelector('td.actions .action-row');
        const wrap = table.closest('.team-list-wrap');
        const metricCells = firstRow ? [...firstRow.querySelectorAll('td:nth-child(2),td:nth-child(3),td:nth-child(4)')].map((cell) => {
          const st = getComputedStyle(cell);
          const r = cell.getBoundingClientRect();
          return { text: (cell.innerText || cell.textContent || '').trim(), width: Math.round(r.width), height: Math.round(r.height), borderStyle: st.borderStyle, borderRadius: st.borderRadius, backgroundColor: st.backgroundColor };
        }) : [];
        return {
          tableDisplay: getComputedStyle(table).display,
          bodyDisplay: tbody ? getComputedStyle(tbody).display : '',
          headDisplay: thead ? getComputedStyle(thead).display : '',
          rowDisplay: firstRow ? getComputedStyle(firstRow).display : '',
          rowColumns: firstRow ? getComputedStyle(firstRow).gridTemplateColumns : '',
          actionDisplay: firstAction ? getComputedStyle(firstAction).display : '',
          metricCells,
          wrapBorder: wrap ? getComputedStyle(wrap).borderStyle : '',
          rows: table.querySelectorAll('tbody tr').length,
        };
      }),
      keyListStats: [...document.querySelectorAll('.key-list-table')].map((table) => {
        const tbody = table.querySelector('tbody');
        const thead = table.querySelector('thead');
        const firstRow = table.querySelector('tbody tr');
        const firstAction = table.querySelector('td.actions .action-row');
        const wrap = table.closest('.key-list-wrap');
        const metricCells = firstRow ? [...firstRow.querySelectorAll('td:nth-child(2),td:nth-child(3),td:nth-child(4)')].map((cell) => {
          const st = getComputedStyle(cell);
          const r = cell.getBoundingClientRect();
          return { text: (cell.innerText || cell.textContent || '').trim(), width: Math.round(r.width), height: Math.round(r.height), borderStyle: st.borderStyle, borderRadius: st.borderRadius, backgroundColor: st.backgroundColor };
        }) : [];
        return {
          tableDisplay: getComputedStyle(table).display,
          bodyDisplay: tbody ? getComputedStyle(tbody).display : '',
          headDisplay: thead ? getComputedStyle(thead).display : '',
          rowDisplay: firstRow ? getComputedStyle(firstRow).display : '',
          rowColumns: firstRow ? getComputedStyle(firstRow).gridTemplateColumns : '',
          actionDisplay: firstAction ? getComputedStyle(firstAction).display : '',
          metricCells,
          wrapBorder: wrap ? getComputedStyle(wrap).borderStyle : '',
          rows: table.querySelectorAll('tbody tr').length,
        };
      }),
      keySecretRows: [...document.querySelectorAll('.key-secret')].map((n) => {
        const st = getComputedStyle(n);
        const key = n.querySelector('.mono');
        const ks = key ? getComputedStyle(key) : null;
        return {
          display: st.display,
          columns: st.gridTemplateColumns,
          keyText: key ? (key.innerText || key.textContent || '').trim() : '',
          copyButtons: n.querySelectorAll('button.icon-btn').length,
          copyLabels: [...n.querySelectorAll('button.icon-btn')].map((b) => ((b.innerText || '').trim() || b.getAttribute('aria-label') || b.title || '').trim()),
          revealButtons: [...n.querySelectorAll('button')].filter((b) => (b.innerText || '').trim() === 'Reveal').length,
          keyScrollHeight: key ? key.scrollHeight : 0,
          keyClientHeight: key ? key.clientHeight : 0,
          keyTextOverflow: ks ? ks.textOverflow : '',
          keyOverflow: ks ? ks.overflow : '',
          keyWhiteSpace: ks ? ks.whiteSpace : '',
        };
      }),
      keyLimitItems: [...document.querySelectorAll('.keys-table .mini-meta-grid span')].map((n) => ({
        text: (n.innerText || n.textContent || '').trim(),
        scrollWidth: n.scrollWidth,
        clientWidth: n.clientWidth,
        scrollHeight: n.scrollHeight,
        clientHeight: n.clientHeight,
      })),
      keyOwnerScopeLabels: [...document.querySelectorAll('.key-list-table td:nth-child(2) .compact-line, .key-list-table td:nth-child(3) .compact-line')].map((n) => {
        const st = getComputedStyle(n);
        return {
          text: (n.innerText || n.textContent || '').trim(),
          className: n.className || '',
          scrollWidth: n.scrollWidth,
          clientWidth: n.clientWidth,
          scrollHeight: n.scrollHeight,
          clientHeight: n.clientHeight,
          whiteSpace: st.whiteSpace,
          overflow: st.overflow,
          textOverflow: st.textOverflow,
        };
      }),
      cardRects: [...document.querySelectorAll('#content .grid .card')].map((n) => {
        const r = n.getBoundingClientRect();
        return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height), text: (n.innerText || '').trim().slice(0, 80) };
      }),
      visibleUsageFilterFields: [...document.querySelectorAll('#content .usage-filter-panel .filter-grid input, #content .usage-filter-panel .filter-grid select')].filter((n) => n.offsetParent !== null).length,
      visibleButtons: [...document.querySelectorAll('button')].filter((b) => b.offsetParent !== null).map((b) => b.innerText.trim()).filter(Boolean).slice(0, 30),
      visibleButtonDetails: [...document.querySelectorAll('button')].filter((b) => b.offsetParent !== null).map((b) => ({ text: b.innerText.trim(), copy: b.dataset.copy || '', title: b.title || '' })).filter((b) => b.text).slice(0, 40),
    };
  });
}

async function verifyCopyFallback(page, name) {
  const evidence = await page.evaluate(async () => {
    const originalExec = document.execCommand;
    let execCalled = false;
    let copiedValue = '';
    document.execCommand = function(cmd) {
      execCalled = cmd === 'copy';
      copiedValue = document.querySelector('textarea')?.value || '';
      return execCalled;
    };
    let restoreClipboard = null;
    const forcedClipboard = { writeText: () => Promise.reject(new Error('forced clipboard failure')) };
    try {
      const desc = Object.getOwnPropertyDescriptor(Navigator.prototype, 'clipboard') || Object.getOwnPropertyDescriptor(navigator, 'clipboard');
      const original = navigator.clipboard;
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: forcedClipboard });
      restoreClipboard = () => {
        try {
          if (desc) Object.defineProperty(Navigator.prototype, 'clipboard', desc);
          Object.defineProperty(navigator, 'clipboard', { configurable: true, value: original });
        } catch {}
      };
    } catch {
      restoreClipboard = null;
    }
    try {
      if (typeof copyText !== 'function') return { hasHelper: false, execCalled, copiedValue, toast: '' };
      copyText('copy-fallback-probe', 'Fallback probe copied');
      await new Promise((resolve) => setTimeout(resolve, 180));
      return {
        hasHelper: true,
        execCalled,
        copiedValue,
        toast: document.querySelector('#toast')?.innerText || '',
      };
    } finally {
      document.execCommand = originalExec;
      if (restoreClipboard) restoreClipboard();
    }
  });
  if (!evidence.hasHelper || !evidence.execCalled || evidence.copiedValue !== 'copy-fallback-probe' || !/Fallback probe copied/i.test(evidence.toast || '')) {
    fail(`${name}: copy fallback should work when navigator.clipboard rejects`, evidence);
  }
}

async function runViewport(browser, name, width, height) {
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width, height } });
  page.on('console', (m) => { if (m.type() === 'error' && !/ERR_NETWORK_CHANGED/i.test(m.text())) result.consoleErrors.push(`${name}: ${m.text()}`); });
  page.on('pageerror', (e) => result.consoleErrors.push(`${name}: pageerror ${e.message}`));
  await login(page);
  await verifyAdminReload(page, name);
  if (width >= 1200) await verifyCopyFallback(page, name);
  const mobile = width <= 820;
  if (mobile) await verifyMobileSidebar(page, name);
  result.pages[name] = {};
  for (const view of ['dashboard', 'providers', 'models', 'teams', 'keys', 'usage', 'settings']) {
    await go(page, view, mobile);
    const shot = `${OUT}/${name}-${view}.png`;
    await page.screenshot({ path: shot, fullPage: true });
    result.screenshots.push(shot);
    const metrics = await inspectPage(page, `${name}-${view}`);
    result.pages[name][view] = metrics;
    if (metrics.bodyScrollWidth > metrics.clientWidth + 8) fail(`${name}/${view}: whole-page horizontal overflow`, metrics);
    if (view !== 'dashboard' && (metrics.visibleToasts || []).some((t) => /Fallback probe copied/i.test(t))) fail(`${name}/${view}: stale copy toast survived navigation`, metrics);
    if (!metrics.title) fail(`${name}/${view}: missing page title`, metrics);
    if (!metrics.panelCount && view !== 'dashboard') fail(`${name}/${view}: no content panels`, metrics);
    if (['providers', 'models', 'teams', 'keys'].includes(view) && (metrics.summaryCards || []).length < 4) fail(`${name}/${view}: missing operational summary cards`, metrics);
    if (view === 'usage') {
      if (!(metrics.visibleButtons || []).includes('Show filters')) fail(`${name}/${view}: Usage should default to collapsed filters with a Show filters action`, metrics);
      for (const label of ['7d', '30d', '90d']) {
        if (!(metrics.visibleButtons || []).includes(label)) fail(`${name}/${view}: Usage collapsed filters should keep ${label} quick range visible`, metrics);
      }
      if (metrics.visibleUsageFilterFields > 0) fail(`${name}/${view}: Usage default filter panel renders too many fields before data`, metrics);
      if (!mobile) {
        const badBreakdowns = (metrics.usageBreakdownStats || []).filter((g) => g.display !== 'grid' || g.cards < 3 || g.tables > 0 || (g.clippedTitles || []).length);
        if (badBreakdowns.length || !(metrics.usageBreakdownStats || []).length) fail(`${name}/${view}: desktop Usage breakdowns should be readable cards, not narrow tables`, { badBreakdowns, metrics });
      }
    }
    if (!mobile && view === 'dashboard') {
      const diag = metrics.diagnosticDetails || [];
      if (!diag.length || diag.some((d) => d.open)) fail(`${name}/${view}: desktop dashboard technical diagnostics should default collapsed`, metrics);
    }
    if (view === 'dashboard') {
      const endpointButton = (metrics.visibleButtonDetails || []).find((b) => b.text === 'Copy endpoint');
      if (!endpointButton || !/\/v1\/chat\/completions$/.test(endpointButton.copy || '')) fail(`${name}/${view}: dashboard must expose a Copy endpoint action with the OpenAI-compatible URL`, { endpointButton, metrics });
    }
    const jsTruncatedLabels = metrics.jsTruncatedLabels || [];
    if (jsTruncatedLabels.length) fail(`${name}/${view}: data labels are shortened in JavaScript instead of CSS/title`, { jsTruncatedLabels, metrics });
    const clippedLegends = (metrics.legendStats || []).filter((l) => l.text && (l.scrollWidth > l.clientWidth + 4 || l.scrollHeight > l.clientHeight + 4));
    if (clippedLegends.length) fail(`${name}/${view}: chart legend text is clipped`, { clippedLegends, metrics });
    if (['dashboard', 'usage'].includes(view)) {
      const badLegendLabels = (metrics.primaryDataLabels || []).filter((l) => /legend-label/.test(l.className || '') && l.text && (/nowrap/i.test(l.whiteSpace) || /ellipsis/i.test(l.textOverflow) || l.scrollWidth > l.clientWidth + 4 || l.scrollHeight > l.clientHeight + 4));
      if (badLegendLabels.length) fail(`${name}/${view}: chart legend labels should wrap instead of truncating`, { badLegendLabels, metrics });
      const badLegendContainers = (metrics.legendContainerStats || []).filter((l) => l.itemCount > 0 && (l.display !== 'flex' || !/auto|scroll/i.test(l.overflowX || '') || (!mobile && l.flexWrap !== 'nowrap') || l.height > (mobile ? 90 : 64)));
      if (badLegendContainers.length) fail(`${name}/${view}: chart legend should be compact horizontal chips with controlled height`, { badLegendContainers, metrics });
    }
    if (['dashboard', 'usage'].includes(view) && (metrics.legendStats || []).length && !(metrics.chartValueLabels || []).some((l) => /^\d|[KMB]/.test(l.text))) {
      fail(`${name}/${view}: sparse chart bars need visible value labels`, metrics);
    }
    if (!mobile && ['dashboard', 'usage'].includes(view) && (metrics.chartDataBars || []).length > 0 && (metrics.chartDataBars || []).length <= 3) {
      const skinnyBars = (metrics.chartDataBars || []).filter((b) => b.width < 44);
      if (skinnyBars.length) fail(`${name}/${view}: sparse desktop chart bars are too skinny to read`, { skinnyBars, metrics });
    }
    if (view === 'dashboard') {
      const speed = (metrics.opsStatus || []).find((s) => s.label === 'Speed');
      if (speed && /\*$/.test(speed.value)) fail(`${name}/${view}: hero Speed KPI must not promote short-sample Tokens/sec values`, { speed, metrics });
      if (speed && /short sample/i.test(speed.note || '')) fail(`${name}/${view}: hero Speed KPI should ask for a longer sample instead of showing short-sample copy`, { speed, metrics });
    }
    if (mobile) {
      const tinyChartLabels = (metrics.chartValueLabels || []).filter((l) => l.text && l.height < 8);
      if (tinyChartLabels.length) fail(`${name}/${view}: mobile chart value labels are too small to read`, { tinyChartLabels, metrics });
      if (['dashboard', 'usage'].includes(view)) {
        const clippedPrimaryLabels = (metrics.primaryDataLabels || []).filter((l) => l.text && (/nowrap/i.test(l.whiteSpace) || /ellipsis/i.test(l.textOverflow) || l.scrollWidth > l.clientWidth + 4 || l.scrollHeight > l.clientHeight + 4));
        if (clippedPrimaryLabels.length) fail(`${name}/${view}: mobile primary data labels should wrap instead of truncating`, { clippedPrimaryLabels, metrics });
        const stretchedStatusPills = (metrics.requestStatusPills || []).filter((p) => p.text && (p.width > 86 || !/start|auto/i.test(String(p.justifySelf))));
        if (stretchedStatusPills.length) fail(`${name}/${view}: mobile request status should be a compact pill, not a stretched bar`, { stretchedStatusPills, metrics });
        const clippedSpeedKpis = (metrics.requestKpis || []).filter((p) => /tokens\/sec/i.test(p.label || '') && p.value && (/ellipsis/i.test(p.textOverflow || '') || p.scrollWidth > p.clientWidth + 4 || p.scrollHeight > p.clientHeight + 4));
        if (clippedSpeedKpis.length) fail(`${name}/${view}: mobile request Tokens/sec value must be fully readable`, { clippedSpeedKpis, metrics });
      }
    }
    const redDisabledDanger = (metrics.disabledDangerButtons || []).filter((b) => /248, 113, 113/.test(b.color) || /248, 113, 113/.test(b.borderColor));
    if (redDisabledDanger.length) fail(`${name}/${view}: disabled destructive actions still look clickable/red`, { redDisabledDanger, metrics });
    if (view === 'providers' && !mobile) {
      const badProviderList = (metrics.providerListStats || []).filter((r) => r.rows > 0 && (r.tableDisplay !== 'block' || r.bodyDisplay !== 'grid' || r.headDisplay !== 'none' || r.rowDisplay !== 'grid' || r.actionDisplay !== 'grid'));
      if (badProviderList.length || !(metrics.providerListStats || []).length) fail(`${name}/${view}: desktop Providers should render as compact provider cards, not a wide sparse table`, { badProviderList, metrics });
      if (width >= 1200) {
        const wrappedProviderActions = (metrics.providerListStats || []).filter((r) => (r.actionButtons || []).length >= 4 && (Math.max(...r.actionButtons.map((b) => b.y)) - Math.min(...r.actionButtons.map((b) => b.y)) > 5));
        if (wrappedProviderActions.length) fail(`${name}/${view}: desktop provider actions should fit on one row`, { wrappedProviderActions, metrics });
        const flatMetricCells = (metrics.providerListStats || []).filter((r) => r.rows > 0 && (r.metricCells || []).some((c) => c.borderStyle === 'none' || parseFloat(c.borderRadius || '0') < 8 || c.height < 44));
        if (flatMetricCells.length) fail(`${name}/${view}: desktop provider metrics should read as compact cards, not flat table cells`, { flatMetricCells, metrics });
      }
    }
    if (['providers', 'models'].includes(view) && (metrics.contentText || '').includes(`${prefix}-Custom LLM`)) {
      fail(`${name}/${view}: generated provider test prefixes should not leak into primary provider labels`, metrics);
    }
    if (view === 'teams' && !mobile) {
      const badTeamList = (metrics.teamListStats || []).filter((r) => r.rows > 0 && (r.tableDisplay !== 'block' || r.bodyDisplay !== 'grid' || r.headDisplay !== 'none' || r.rowDisplay !== 'grid' || r.actionDisplay !== 'grid'));
      if (badTeamList.length || !(metrics.teamListStats || []).length) fail(`${name}/${view}: desktop Teams should render as compact team cards, not a wide sparse table`, { badTeamList, metrics });
      const flatTeamMetricCells = (metrics.teamListStats || []).filter((r) => r.rows > 0 && (r.metricCells || []).some((c) => c.borderStyle === 'none' || parseFloat(c.borderRadius || '0') < 8 || c.height < 44));
      if (flatTeamMetricCells.length) fail(`${name}/${view}: desktop team budget/key/status values should read as compact cards, not flat table cells`, { flatTeamMetricCells, metrics });
    }
    if (view === 'keys' && !mobile) {
      const badKeyList = (metrics.keyListStats || []).filter((r) => r.rows > 0 && (r.tableDisplay !== 'block' || r.bodyDisplay !== 'grid' || r.headDisplay !== 'none' || r.rowDisplay !== 'grid' || r.actionDisplay !== 'grid'));
      if (badKeyList.length || !(metrics.keyListStats || []).length) fail(`${name}/${view}: desktop API Keys should render as compact key cards, not a wide sparse table`, { badKeyList, metrics });
      const flatKeyMetricCells = (metrics.keyListStats || []).filter((r) => r.rows > 0 && (r.metricCells || []).some((c) => c.borderStyle === 'none' || parseFloat(c.borderRadius || '0') < 8 || c.height < 50));
      if (flatKeyMetricCells.length) fail(`${name}/${view}: desktop API key owner/scope/limit values should read as compact cards, not flat table cells`, { flatKeyMetricCells, metrics });
    }
    if (view === 'models') {
      const badModelNameRows = (metrics.modelNameRows || []).filter((r) => r.display !== 'grid' || r.copyButtons !== 1);
      if (badModelNameRows.length) fail(`${name}/${view}: public model name and copy action are not aligned as a stable grid`, { badModelNameRows, metrics });
      const clippedModelNames = (metrics.modelNameRows || []).filter((r) => /ellipsis/i.test(r.nameTextOverflow) || r.nameLineClamp !== 'none' || r.nameOverflow !== 'visible' || r.nameScrollHeight > r.nameClientHeight + 4);
      if (clippedModelNames.length) fail(`${name}/${view}: public model names are clipped`, { clippedModelNames, metrics });
      if (!mobile) {
        const badRouteList = (metrics.routeListStats || []).filter((r) => r.rows > 0 && (r.tableDisplay !== 'block' || r.bodyDisplay !== 'grid' || r.headDisplay !== 'none' || r.rowDisplay !== 'grid' || r.actionDisplay !== 'grid'));
        if (badRouteList.length || !(metrics.routeListStats || []).length) fail(`${name}/${view}: desktop Models should render as compact route cards, not a wide sparse table`, { badRouteList, metrics });
        const narrowPublicNameCells = (metrics.routeListStats || []).filter((r) => r.rows > 0 && width >= 1200 && r.publicNameCellWidth < 300);
        if (narrowPublicNameCells.length) fail(`${name}/${view}: desktop public model column is too narrow for real provider names`, { narrowPublicNameCells, metrics });
        const hiddenRouteActions = (metrics.routeListStats || []).filter((r) => r.rows > 0 && ((r.actionButtons || []).length < 3 || (r.actionButtons || []).some((b) => !b.visible || b.right > (r.wrapRight || width) + 2 || b.width < 52)));
        if (hiddenRouteActions.length) fail(`${name}/${view}: desktop route actions must remain visible inside each route card`, { hiddenRouteActions, metrics });
        const flatRouteMetricCells = (metrics.routeListStats || []).filter((r) => r.rows > 0 && (r.metricCells || []).some((c) => c.borderStyle === 'none' || parseFloat(c.borderRadius || '0') < 8 || c.height < 44));
        if (flatRouteMetricCells.length) fail(`${name}/${view}: desktop route details should read as compact cards, not flat table cells`, { flatRouteMetricCells, metrics });
      }
    }
    if (view === 'keys') {
      const badKeyRows = (metrics.keySecretRows || []).filter((r) => r.keyText && r.keyText !== 'legacy · recreate' && (r.keyText === '…' || r.copyButtons !== 1 || r.revealButtons !== 0));
      if (badKeyRows.length) fail(`${name}/${view}: visible API key rows should show full key plus copy, without redundant Reveal action`, { badKeyRows, metrics });
      const unclearCopyKeys = (metrics.keySecretRows || []).filter((r) => r.keyText && r.keyText !== 'legacy · recreate' && !(r.copyLabels || []).some((label) => /copy key|copy api key/i.test(label)));
      if (unclearCopyKeys.length) fail(`${name}/${view}: API key copy action should be explicitly labelled`, { unclearCopyKeys, metrics });
      const clippedKeys = (metrics.keySecretRows || []).filter((r) => r.keyText && r.keyText !== 'legacy · recreate' && (/ellipsis/i.test(r.keyTextOverflow) || r.keyOverflow !== 'visible' || r.keyScrollHeight > r.keyClientHeight + 4));
      if (clippedKeys.length) fail(`${name}/${view}: revealed API keys are clipped`, { clippedKeys, metrics });
      const clippedKeyLimits = (metrics.keyLimitItems || []).filter((r) => r.text && (r.scrollWidth > r.clientWidth + 4 || r.scrollHeight > r.clientHeight + 4));
      if (clippedKeyLimits.length) fail(`${name}/${view}: API key limit labels are clipped`, { clippedKeyLimits, metrics });
      const clippedKeyOwnerScope = (metrics.keyOwnerScopeLabels || []).filter((r) => r.text && (/nowrap/i.test(r.whiteSpace || '') || /ellipsis/i.test(r.textOverflow || '') || r.scrollWidth > r.clientWidth + 4 || r.scrollHeight > r.clientHeight + 4));
      if (clippedKeyOwnerScope.length) fail(`${name}/${view}: API key owner/team/scope labels are clipped`, { clippedKeyOwnerScope, metrics });
      if (!mobile) {
        const wrappedDesktopKeys = (metrics.keySecretRows || []).filter((r) => r.keyText && r.keyText !== 'legacy · recreate' && r.keyClientHeight > 24);
        if (wrappedDesktopKeys.length) fail(`${name}/${view}: desktop API keys should fit on one readable line`, { wrappedDesktopKeys, metrics });
        const ghostKeyColumns = (metrics.keySecretRows || []).filter((r) => r.keyText && r.keyText !== 'legacy · recreate' && /0px/.test(r.columns || ''));
        if (ghostKeyColumns.length) fail(`${name}/${view}: desktop API key rows should not reserve a ghost Reveal column`, { ghostKeyColumns, metrics });
      }
    }
    if (mobile) {
      if (view === 'dashboard' && metrics.cards >= 4) {
        const firstRow = (metrics.cardRects || []).filter((r) => Math.abs(r.y - metrics.cardRects[0].y) <= 4);
        if (firstRow.length < 2) fail(`${name}/${view}: mobile KPI cards should use a compact two-column layout`, { cardRects: metrics.cardRects, metrics });
        const crampedCards = (metrics.cardRects || []).filter((r) => r.width < 130);
        if (crampedCards.length) fail(`${name}/${view}: mobile KPI cards are too narrow to read`, { crampedCards, metrics });
        const diag = metrics.diagnosticDetails || [];
        if (!diag.length || diag.some((d) => d.open)) fail(`${name}/${view}: mobile dashboard technical diagnostics should default collapsed`, metrics);
        const recentY = (metrics.sectionPositions || []).find((s) => s.text === 'Recent requests')?.y;
        const diagY = (metrics.sectionPositions || []).find((s) => s.text === 'Technical diagnostics')?.y;
        if (recentY == null || diagY == null || recentY > diagY) fail(`${name}/${view}: mobile Recent requests should appear before technical diagnostics`, metrics);
      }
      if (view === 'usage') {
        const breakdown = (metrics.diagnosticDetails || []).filter((d) => /Usage breakdowns/i.test(d.text || ''));
        if (!breakdown.length || breakdown.some((d) => d.open)) fail(`${name}/${view}: mobile Usage breakdowns should default collapsed`, metrics);
        const logsY = (metrics.sectionPositions || []).find((s) => s.text === 'Request logs')?.y;
        const breakdownY = (metrics.sectionPositions || []).find((s) => s.text === 'Usage breakdowns')?.y;
        if (logsY == null || breakdownY == null || logsY > breakdownY) fail(`${name}/${view}: mobile Request logs should appear before usage breakdowns`, metrics);
        await page.getByRole('button', { name: 'Show filters' }).click({ force: true });
        await page.waitForTimeout(250);
        const expandedFilterFields = await page.evaluate(() => [...document.querySelectorAll('#content .usage-filter-panel .filter-grid input, #content .usage-filter-panel .filter-grid select')].filter((n) => n.offsetParent !== null).length);
        if (expandedFilterFields < 7) fail(`${name}/${view}: Show filters did not expand the full Usage filter form`, { expandedFilterFields, metrics });
      }
      const wideTables = metrics.tableStats.filter((t) => t.scrollWidth > t.clientWidth + 8);
      if (wideTables.length) fail(`${name}/${view}: mobile table still scrolls horizontally`, { wideTables, metrics });
      const rowTablesWithoutLabels = metrics.tableStats.filter((t) => t.rows > 0 && t.cells > 0 && t.labelledCells < Math.max(1, t.cells - t.rows));
      if (rowTablesWithoutLabels.length) fail(`${name}/${view}: mobile table rows lack readable column labels`, { rowTablesWithoutLabels, metrics });
      const tableModeFailures = metrics.tableStats.filter((t) => t.rows > 0 && (t.tableDisplay !== 'block' || t.bodyDisplay !== 'grid'));
      if (tableModeFailures.length) fail(`${name}/${view}: mobile tables are not rendered as stacked cards`, { tableModeFailures, metrics });
      const cellLayoutFailures = metrics.tableStats.filter((t) => t.rows > 0 && (t.sampleCellDisplay !== 'block' || t.sampleCellTextAlign !== 'left' || t.sampleBeforeDisplay !== 'block'));
      if (cellLayoutFailures.length) fail(`${name}/${view}: mobile table cells must show label above left-aligned value`, { cellLayoutFailures, metrics });
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
