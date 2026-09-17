import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const stamp = Date.now().toString().slice(-7);

const result = { result: 'FAIL', failures: [], bugs: [], evidence: {}, consoleErrors: [] };
function bug(message) { result.bugs.push(message); }
function fail(message) { result.failures.push(message); }

function compact(v) {
  if (v == null || Number.isNaN(Number(v))) return '—';
  const n = Math.round(Number(v));
  if (Math.abs(n) >= 1_000_000_000) return Math.round(n / 1_000_000_000) + 'B';
  if (Math.abs(n) >= 1_000_000) return Math.round(n / 1_000_000) + 'M';
  if (Math.abs(n) >= 1_000) return Math.round(n / 1_000) + 'K';
  return String(n);
}


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
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status} ${text.slice(0, 200)}`);
  return res.status === 204 || !text ? null : JSON.parse(text);
}
async function clientFetch(path, key, method = 'POST', body) {
  const res = await fetch(BASE + path, {
    method,
    headers: { 'content-type': 'application/json', authorization: 'Bearer ' + key },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return { status: res.status, text: await res.text() };
}

async function login(page) {
  await gotoWithRetry(page, BASE + '/');
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
  return page.locator('.modal .field')
    .filter({ has: page.locator('label', { hasText: label }) })
    .first()
    .locator('input,textarea,select')
    .first();
}
async function fill(page, label, value) { await (await field(page, label)).fill(String(value)); }
async function row(page, text) {
  const literal = String(text).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  return page.locator('tr').filter({ has: page.locator(`[title="${literal}"], [aria-label="${literal}"]`) }).first()
    .or(page.locator('tr').filter({ hasText: text }).first())
    .first();
}
async function panels(page, text) { return page.locator('.panel').filter({ hasText: text }).count(); }
async function modalButton(page, name) { return page.locator('.modal').getByRole('button', { name }).first(); }
async function clickRowButton(page, rowText, buttonName) {
  const r = await row(page, rowText);
  await r.waitFor({ state: 'visible', timeout: 7000 });
  await r.getByRole('button', { name: buttonName }).first().click({ force: true });
}

async function seedUsage(page) {
  const model = `pw-polish-usage-public-route-with-long-readable-name-${stamp}`;
  let backendId = null;
  let keyId = null;
  try {
    try { await adminFetch('/admin/routes/' + encodeURIComponent(model), 'DELETE'); } catch {}
    const existing = await adminFetch('/admin/backends');
    for (const b of existing) {
      if (String(b.name || '').startsWith('pw-polish-usage-') && b.can_delete !== false) {
        try { await adminFetch('/admin/backends/' + b.id, 'DELETE'); } catch {}
      }
    }
    const backend = await adminFetch('/admin/backends', 'POST', {
      name: `pw-polish-usage-provider-connection-with-long-readable-name-${stamp}`,
      base_url: 'http://127.0.0.1:9000/v1',
      api_key_ref: 'env:NONE',
      weight: 1,
      max_inflight: 0,
      format: 'openai',
      enabled: true,
    });
    backendId = backend.id;
    await adminFetch('/admin/routes', 'POST', {
      model_name: model,
      backend_ids: [backendId],
      provider_model_name: 'mock-model',
      auth_mode: 'none',
      protocol: 'local_openai_chat',
      enabled: true,
      chars_per_token: 4,
      first_byte_timeout: 180,
    });
    const key = await adminFetch('/admin/keys', 'POST', { team_id: 1, owner: `pw-polish-usage-owner-with-long-readable-name-${stamp}`, allowed_models: [model], budget: null, expires_at: null });
    keyId = key.id;
    const revealed = await adminFetch('/admin/keys/' + keyId + '/reveal');
    const resp = await clientFetch('/v1/chat/completions', revealed.key, 'POST', {
      model,
      messages: [{ role: 'user', content: 'Reply OK' }],
      max_tokens: 8,
      stream: false,
    });
    result.evidence.usageSmokeStatus = resp.status;
    if (resp.status !== 200) bug(`Usage smoke request failed; status=${resp.status}, body=${resp.text.slice(0, 160)}`);
    let smokeUsageRow = null;
    for (let i = 0; i < 10; i++) {
      const rows = await adminFetch('/admin/usage?days=7');
      smokeUsageRow = rows.find((r) => r.model === model) || null;
      if (smokeUsageRow) break;
      await page.waitForTimeout(500);
    }
    result.evidence.usageSmokeRow = smokeUsageRow;
    if (!smokeUsageRow) bug('Usage smoke request was not written to request logs.');
    else if ((smokeUsageRow.output_tokens || 0) > 0 && smokeUsageRow.total_tokens_per_second == null) bug('Usage log row has output tokens but total_tokens_per_second is null.');
    return { model, backendId, keyId };
  } catch (e) {
    bug(`Could not seed usage smoke data: ${e.message}`);
    return { model, backendId, keyId };
  }
}

async function main() {
  if (!ADMIN) fail('BRIGHTO_ADMIN_KEY is required');
  if (result.failures.length) return;

  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  const browser = await chromium.launch(launch);
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, ignoreHTTPSErrors: true });
  page.on('console', (m) => { if (m.type() === 'error' && !/ERR_NETWORK_CHANGED/i.test(m.text())) result.consoleErrors.push(m.text()); });
  page.on('pageerror', (e) => result.consoleErrors.push('pageerror:' + e.message));
  page.on('dialog', async (d) => { result.evidence.lastDialog = d.message(); await d.accept(); });

  let usageSeed = null;
  try {
    await login(page);
    result.evidence.loginOk = true;

    result.evidence.formatters = await page.evaluate(() => ({
      fmt1000: typeof fmt === 'function' ? fmt(1000) : null,
      fmt50000: typeof fmt === 'function' ? fmt(50000) : null,
      fmt1000000: typeof fmt === 'function' ? fmt(1000000) : null,
      fmtCount1000: typeof fmtCount === 'function' ? fmtCount(1000) : null,
      fmtCount50000: typeof fmtCount === 'function' ? fmtCount(50000) : null,
      fmtCount1000000: typeof fmtCount === 'function' ? fmtCount(1000000) : null,
      fmtNum1000: typeof fmtNum === 'function' ? fmtNum(1000) : null,
      fmtNum1000000: typeof fmtNum === 'function' ? fmtNum(1000000) : null,
      fmtDur0: typeof fmtDur === 'function' ? fmtDur(0) : null,
    }));
    if (result.evidence.formatters.fmtCount1000 !== '1K' || result.evidence.formatters.fmtCount50000 !== '50K' || result.evidence.formatters.fmtCount1000000 !== '1M') bug(`fmtCount examples wrong: ${JSON.stringify(result.evidence.formatters)}`);
    if (result.evidence.formatters.fmt1000 !== '1K') bug(`Default count formatter must use K/M/B: fmt(1000)=${result.evidence.formatters.fmt1000}`);
    if (String(result.evidence.formatters.fmtNum1000).includes('k')) bug(`Chart formatter must use uppercase K: fmtNum(1000)=${result.evidence.formatters.fmtNum1000}`);
    if (result.evidence.formatters.fmtDur0 === '0 ms') bug('Duration formatter must not show 0 ms; use <1 ms for sub-millisecond work.');

    await page.locator('.card .big').first().waitFor({ state: 'visible', timeout: 7000 }).catch(() => null);
    result.evidence.dashboardDensity = await page.evaluate(() => {
      const title = document.querySelector('.topbar h2');
      const cardBig = document.querySelector('.card .big');
      const panel = document.querySelector('.panel');
      const card = document.querySelector('.card');
      return {
        bodyFont: getComputedStyle(document.body).fontSize,
        titleFont: title ? getComputedStyle(title).fontSize : null,
        cardBig: cardBig ? getComputedStyle(cardBig).fontSize : null,
        panelPadding: panel ? getComputedStyle(panel).padding : null,
        panelMargin: panel ? getComputedStyle(panel).marginBottom : null,
        cardHeight: card ? Math.round(card.getBoundingClientRect().height) : null,
      };
    });
    const dashboardText = await page.locator('#content').innerText();
    result.evidence.dashboardText = dashboardText.slice(0, 1600);
    if (!/Speed|Tokens\/sec/i.test(dashboardText)) bug('Dashboard hero must surface request speed / Tokens/sec, not hide it in logs only.');
    const clippedHeroSubtexts = await page.evaluate(() => [...document.querySelectorAll('.ops-status .cell-sub')].filter((n) => n.scrollWidth > n.clientWidth + 4).map((n) => ({ text: n.innerText, scrollWidth: n.scrollWidth, clientWidth: n.clientWidth })));
    result.evidence.clippedHeroSubtexts = clippedHeroSubtexts;
    if (clippedHeroSubtexts.length) bug('Dashboard hero KPI subtext must wrap cleanly instead of truncating.');
    if (!result.evidence.dashboardDensity.cardBig) fail('Dashboard did not render metric cards after login.');
    else if (parseFloat(result.evidence.dashboardDensity.cardBig) >= 30) bug(`Dashboard big-number font too large: ${result.evidence.dashboardDensity.cardBig}`);
    if (!result.evidence.dashboardDensity.panelPadding) fail('Dashboard did not render any panel after login.');
    else if (parseFloat(result.evidence.dashboardDensity.panelPadding) >= 20) bug(`Panel padding too airy: ${result.evidence.dashboardDensity.panelPadding}`);

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
    if (result.evidence.rootPrefs.htmlDensity !== 'compact') bug(`Default density must be compact; got ${result.evidence.rootPrefs.htmlDensity}`);
    if (!result.evidence.rootPrefs.localStorageKeys.length) bug('Portal preferences are not persisted in localStorage after initial apply.');

    await nav(page, 'Teams');
    const teamSummary = await page.locator('.summary-card').count();
    result.evidence.teamSummaryCards = teamSummary;
    if (teamSummary < 4) bug(`Teams page must show operational summary cards; got ${teamSummary}.`);

    await nav(page, 'Providers');
    const providerSummary = await page.locator('.summary-card').count();
    result.evidence.providerSummaryCards = providerSummary;
    if (providerSummary < 4) bug(`Providers page must show operational summary cards; got ${providerSummary}.`);
    const providerButtons = await page.locator('#content button').evaluateAll((nodes) => nodes.map((b) => (b.innerText || '').trim()).filter(Boolean));
    result.evidence.providerButtons = providerButtons;
    if (!providerButtons.includes('Prepare connection')) bug('Providers page must label manual provider creation as Prepare connection.');
    if (providerButtons.includes('Add provider') || providerButtons.includes('Pre-register provider')) bug('Providers page primary CTA must not imply admins need to add providers before models or expose pre-registration jargon.');
    const providerActionPriority = await page.locator('#content tbody tr').first().evaluate((tr) => {
      const out = {};
      for (const b of tr.querySelectorAll('button')) out[(b.innerText || '').trim()] = b.className;
      return out;
    }).catch(() => ({}));
    result.evidence.providerActionPriority = providerActionPriority;
    if (providerActionPriority.Route && !/\bprimary\b/.test(providerActionPriority.Route)) bug('Provider row Route action must be visually primary.');
    if (providerActionPriority.Edit && /\bprimary\b/.test(providerActionPriority.Edit)) bug('Provider row Edit action must not be visually primary.');
    const providerName = `pw-polish-provider-${stamp}`;
    const providerEdit = `${providerName}-edited`;
    await page.getByRole('button', { name: /Prepare connection|Pre-register provider|Add provider/ }).first().click({ force: true });
    await fill(page, 'Name', providerName);
    await fill(page, 'Base URL', 'http://127.0.0.1:65531/v1');
    const providerModalText = await page.locator('.modal').innerText();
    if (!/Prepare provider connection|Prepare a connection/i.test(providerModalText)) bug('Provider modal must explain manual creation as optional connection preparation.');
    if (/Provider Type/i.test(providerModalText)) bug('Provider modal must not expose stale Provider Type jargon.');
    await page.locator('.modal').getByRole('button', { name: /Optional load control/ }).click({ force: true });
    await fill(page, 'Weight', '1');
    await fill(page, 'Simultaneous calls', '0');
    await (await modalButton(page, 'Prepare connection')).click({ force: true });
    await (await row(page, providerName)).waitFor({ state: 'visible', timeout: 8000 });
    let providerPanels = await panels(page, 'Provider connections');
    result.evidence.providerPanelsAfterCreate = providerPanels;
    if (providerPanels !== 1) bug(`Provider create leaves ${providerPanels} Provider connections panels; expected exactly 1.`);

    await nav(page, 'Providers');
    await clickRowButton(page, providerName, 'Edit');
    await fill(page, 'Name', providerEdit);
    await (await modalButton(page, 'Save')).click({ force: true });
    await page.waitForTimeout(800);
    providerPanels = await panels(page, 'Provider connections');
    result.evidence.providerPanelsAfterEdit = providerPanels;
    if (providerPanels !== 1) bug(`Provider edit leaves ${providerPanels} Provider connections panels; expected exactly 1.`);

    await nav(page, 'Providers');
    await clickRowButton(page, providerEdit, 'Delete');
    await page.waitForTimeout(1000);
    providerPanels = await panels(page, 'Provider connections');
    result.evidence.providerPanelsAfterDelete = providerPanels;
    if (providerPanels !== 1) bug(`Provider delete leaves ${providerPanels} Provider connections panels; expected exactly 1.`);

    usageSeed = await seedUsage(page);
    await page.evaluate(() => refresh());
    await page.waitForTimeout(800);

    await nav(page, 'API Keys');
    result.evidence.keyCompactLines = await page.evaluate(() => [...document.querySelectorAll('.key-list-table .compact-line')].map((n) => {
      const r = n.getBoundingClientRect();
      const cs = getComputedStyle(n);
      return { text: (n.textContent || '').trim(), title: n.getAttribute('title') || '', height: Math.round(r.height), clientWidth: n.clientWidth, scrollWidth: n.scrollWidth, whiteSpace: cs.whiteSpace, overflow: cs.overflow, textOverflow: cs.textOverflow, wordBreak: cs.wordBreak, overflowWrap: cs.overflowWrap };
    }));
    const brokenKeyCompact = result.evidence.keyCompactLines.filter((n) => n.height > 24 || n.whiteSpace !== 'nowrap' || n.overflow !== 'hidden' || n.textOverflow !== 'ellipsis' || n.wordBreak !== 'normal' || n.overflowWrap !== 'normal');
    if (brokenKeyCompact.length) bug(`API Keys page owner/team/scope labels must stay one-line ellipsis/copy/title, not clipped or broken wraps: ${JSON.stringify(brokenKeyCompact.slice(0, 4))}`);

    await nav(page, 'Models & Routes');
    result.evidence.modelCompactLines = await page.evaluate(() => [...document.querySelectorAll('.route-list-table .compact-line')].map((n) => {
      const r = n.getBoundingClientRect();
      const cs = getComputedStyle(n);
      return { text: (n.textContent || '').trim(), title: n.getAttribute('title') || '', height: Math.round(r.height), clientWidth: n.clientWidth, scrollWidth: n.scrollWidth, whiteSpace: cs.whiteSpace, overflow: cs.overflow, textOverflow: cs.textOverflow, wordBreak: cs.wordBreak, overflowWrap: cs.overflowWrap };
    }));
    const brokenModelCompact = result.evidence.modelCompactLines.filter((n) => n.height > 24 || n.whiteSpace !== 'nowrap' || n.overflow !== 'hidden' || n.textOverflow !== 'ellipsis' || n.wordBreak !== 'normal' || n.overflowWrap !== 'normal');
    if (brokenModelCompact.length) bug(`Models page long labels must stay one-line ellipsis/copy/title, not broken wraps: ${JSON.stringify(brokenModelCompact.slice(0, 4))}`);

    await nav(page, 'Providers');
    result.evidence.providerCompactLines = await page.evaluate(() => [...document.querySelectorAll('.provider-list-table .compact-line')].map((n) => {
      const r = n.getBoundingClientRect();
      const cs = getComputedStyle(n);
      return { text: (n.textContent || '').trim(), title: n.getAttribute('title') || '', height: Math.round(r.height), clientWidth: n.clientWidth, scrollWidth: n.scrollWidth, whiteSpace: cs.whiteSpace, overflow: cs.overflow, textOverflow: cs.textOverflow, wordBreak: cs.wordBreak, overflowWrap: cs.overflowWrap };
    }));
    const brokenProviderCompact = result.evidence.providerCompactLines.filter((n) => n.height > 24 || n.whiteSpace !== 'nowrap' || n.overflow !== 'hidden' || n.textOverflow !== 'ellipsis' || n.wordBreak !== 'normal' || n.overflowWrap !== 'normal');
    if (brokenProviderCompact.length) bug(`Providers page long labels must stay one-line ellipsis/copy/title, not broken wraps: ${JSON.stringify(brokenProviderCompact.slice(0, 4))}`);

    await nav(page, 'Usage');
    const usageText = await page.locator('#content').innerText();
    result.evidence.usageText = usageText.slice(0, 2500);
    if (!/Tokens\/sec|tokens per second/i.test(usageText)) bug('Usage/logs must visibly prioritize Tokens/sec when requests exist.');
    const usageModelFound = usageSeed.model ? await page.locator(`[title="${usageSeed.model}"], [aria-label="${usageSeed.model}"]`).count() : 0;
    result.evidence.usageModelFoundByTitleOrAria = usageModelFound;
    if (usageSeed.model && !usageText.includes(usageSeed.model) && usageModelFound < 1) bug('Usage page did not show the smoke request model as visible text, title, or aria-label.');
    const smokeLogLine = usageText.split('\n').find((line) => usageSeed.model && (line.includes(usageSeed.model) || line.includes('pw-polish-usage-public'))) || '';
    result.evidence.usageSmokeLine = smokeLogLine;
    const rowMs = result.evidence.usageSmokeRow ? Number(result.evidence.usageSmokeRow.total_ms) : null;
    const expectedTokS = result.evidence.usageSmokeRow && result.evidence.usageSmokeRow.total_tokens_per_second != null
      ? compact(result.evidence.usageSmokeRow.total_tokens_per_second)
      : null;
    result.evidence.expectedTokS = expectedTokS || 'missing-api-token-rate';
    if (expectedTokS && Number.isFinite(rowMs) && rowMs < 1000) {
      if (!/TOKENS\/SEC\s+Too short/i.test(usageText) && !usageText.includes('Too short')) bug('Usage page must show Too short for sub-1s token-rate samples instead of an inflated Tokens/sec number.');
      if (usageText.includes(expectedTokS + '*')) bug('Usage page must not show star-marked inflated Tokens/sec for very short requests.');
    } else if (expectedTokS && !usageText.includes(expectedTokS)) {
      bug(`Usage page did not render compact Tokens/sec value ${expectedTokS}.`);
    }
    if (expectedTokS && Number.isFinite(rowMs) && rowMs >= 1000 && /TOKENS\/SEC\s+—/.test(usageText)) bug('Usage page hides Tokens/sec as — even though the API returned a sustained token-rate value.');
    if (/\b(ROUTER|DURATION)\s+0 ms\b/.test(usageText)) bug('Usage log must show <1 ms instead of 0 ms for sub-millisecond timings.');
    if (/\b\d{1,3},\d{3}\b/.test(usageText)) bug('Usage still shows comma-formatted large counts; expected compact K/M/B display.');
    if (result.consoleErrors.length) bug(`Browser console errors: ${JSON.stringify(result.consoleErrors)}`);
  } catch (e) {
    fail(e.stack || e.message);
  } finally {
    if (usageSeed) {
      try { await adminFetch('/admin/keys/' + usageSeed.keyId, 'DELETE'); } catch {}
      try { await adminFetch('/admin/routes/' + encodeURIComponent(usageSeed.model), 'DELETE'); } catch {}
      try { await adminFetch('/admin/backends/' + usageSeed.backendId, 'DELETE'); } catch {}
    }
    await browser.close();
  }
}

await main();
result.result = (result.failures.length || result.bugs.length) ? 'FAIL' : 'PASS';
await writeFile(`${OUT}/summary.json`, JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
