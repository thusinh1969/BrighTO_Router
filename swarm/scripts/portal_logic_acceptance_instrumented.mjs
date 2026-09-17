import { chromium } from 'playwright';
import { writeFile } from 'fs/promises';

const BASE = process.env.BRIGHTO_BASE_URL || 'https://127.0.0.1:18443';
const ADMIN = process.env.BRIGHTO_ADMIN_KEY;
const OUT = process.env.BRIGHTO_PW_OUT || process.cwd();
const EXECUTABLE = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const stamp = Date.now().toString().slice(-8);
const prefix = `pw-logic-${stamp}`;

const result = {
  result: 'FAIL',
  base: BASE,
  prefix,
  failures: [],
  passes: [],
  evidence: {},
  cleanup: [],
  consoleErrors: [],
};

function pass(area, summary, evidence = {}) {
  console.log(`[PASS] ${area}: ${summary}`);
  result.passes.push({ area, summary, evidence });
}
function fail(area, summary, evidence = {}, requiredFix = '') {
  console.log(`[FAIL] ${area}: ${summary}`);
  result.failures.push({ area, summary, evidence, requiredFix });
}
function redact(k) { return k ? `${k.slice(0, 9)}…${k.slice(-4)}` : k; }

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

function modalField(page, label) {
  return page
    .locator('.modal .field')
    .filter({ has: page.locator('label', { hasText: label }) })
    .locator('input,select,textarea')
    .first();
}
function row(page, text) { return page.locator('tr').filter({ hasText: text }).first(); }
async function ensureProviderLoadControlOpen(page) {
  if (!(await modalField(page, 'Weight').isVisible().catch(() => false))) {
    await page.locator('.modal').getByRole('button', { name: /Optional load control/ }).click({ force: true });
    await modalField(page, 'Weight').waitFor({ state: 'visible', timeout: 3000 });
  }
}
async function toastText(page, ms = 1600) { await page.waitForTimeout(ms); return await page.locator('#toast').innerText().catch(() => ''); }

async function loginAdmin(page) {
  await page.goto(BASE + '/', { waitUntil: 'domcontentloaded', timeout: 20000 });
  await page.locator('#login-user').fill('admin');
  await page.locator('#login-pass').fill(ADMIN);
  await page.evaluate(() => login());
  await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 12000 });
}
async function nav(page, view) {
  await page.locator(`.nav[data-view="${view}"]`).click({ force: true });
  await page.waitForTimeout(700);
}
async function providerOptions(page) {
  return await modalField(page, 'Provider').locator('option').evaluateAll((os) => os.map((o) => ({ text: o.textContent.trim(), value: o.value, disabled: o.disabled })));
}
async function visibleEditAction(page, width) {
  await page.setViewportSize({ width, height: 760 });
  await page.waitForTimeout(300);
  return await page.evaluate(() => {
    const btn = [...document.querySelectorAll('tbody tr:first-child button')].find((b) => b.textContent.trim() === 'Edit');
    const r = btn?.getBoundingClientRect();
    return { viewport: innerWidth, x: r && Math.round(r.x), right: r && Math.round(r.right), visible: !!btn && !!(btn.offsetWidth || btn.offsetHeight || btn.getClientRects().length) };
  });
}

async function main() {
  if (!ADMIN) {
    fail('environment', 'BRIGHTO_ADMIN_KEY is required');
    return;
  }
  const launch = { headless: true };
  if (EXECUTABLE) launch.executablePath = EXECUTABLE;
  const browser = await chromium.launch(launch);
  const page = await browser.newPage({ ignoreHTTPSErrors: true, viewport: { width: 1280, height: 850 } });
  page.on('console', (m) => { if (m.type() === 'error' && !/ERR_NETWORK_CHANGED|ERR_CONNECTION_RESET|ERR_HTTP2_PROTOCOL_ERROR/i.test(m.text())) result.consoleErrors.push(m.text()); });
  page.on('pageerror', (e) => result.consoleErrors.push(`pageerror: ${e.message}`));
  page.on('dialog', async (d) => { result.evidence.lastDialog = d.message(); await d.accept(); });

  let providerId = null;
  let uiProviderId = null;
  let routeName = `${prefix}-route`;
  let teamId = null;
  let keyId = null;

  try {
    await loginAdmin(page);
    pass('auth', 'Admin login works over live HTTPS');

    const fmt = await page.evaluate(() => ({ fmt1000: fmt(1000), fmt50000: fmt(50000), fmt1m: fmt(1000000), density: document.documentElement.dataset.density }));
    if (fmt.fmt1000 === '1K' && fmt.fmt50000 === '50K' && fmt.fmt1m === '1M' && fmt.density === 'compact') pass('polish', 'compact formatter and density are live', fmt);
    else fail('polish', 'formatter/density wrong', fmt, 'Live Docker must serve compact formatter and compact default.');

    let backends = await adminFetch('/admin/backends');
    const isExternalTestName = (name) => /^(pw-|crud-|verify-)/.test(name || '') && !(name || '').startsWith(prefix);
    const preJunk = backends.filter((b) => isExternalTestName(b.name));
    if (preJunk.length) fail('test data', 'test provider rows remain before acceptance', { rows: preJunk.map((b) => ({ id: b.id, name: b.name })) }, 'Acceptance must clean test data before claiming done.');
    else pass('test data', 'no pw/crud/verify provider rows before test');

    await nav(page, 'providers');
    const providerText = await page.locator('#content').innerText();
    result.evidence.providersExcerpt = providerText.slice(0, 1200);
    if (/set-key|edit the key reference here/i.test(providerText)) fail('providers', 'old provider key copy visible', { excerpt: providerText.slice(0, 400) }, 'Remove provider key shell/key-reference copy.');
    else pass('providers', 'provider copy says key belongs to route');

    const openaiLoadButtons = await row(page, 'openai').getByRole('button', { name: 'Load models' }).count().catch(() => 0);
    if (openaiLoadButtons > 0) fail('providers', 'cloud Provider row still has active Load models', { openaiLoadButtons }, 'Remove/disable Provider-row Load models for cloud providers; route wizard owns keyed model loading.');
    else pass('providers', 'cloud Provider row has no active Load models');

    for (const w of [1280, 1024]) {
      const ev = await visibleEditAction(page, w);
      if (ev.visible && ev.x >= 0 && ev.right <= ev.viewport) pass('providers', `Edit action visible at ${w}px`, ev);
      else fail('providers', `Edit action not visible at ${w}px`, ev, 'Sticky/right Provider action must be inside viewport.');
    }

    await page.setViewportSize({ width: 1280, height: 850 });
    await row(page, 'openai').getByRole('button', { name: 'Edit' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    const labels = await page.locator('.modal label').evaluateAll((ls) => ls.map((l) => l.textContent.trim()));
    if (labels.includes('Provider Type')) pass('providers', 'Provider Type exists in Provider modal', { labels });
    else fail('providers', 'Provider Type missing in Provider modal', { labels }, 'Add Provider Type dropdown.');
    await page.locator('.modal').getByRole('button', { name: 'Cancel' }).click({ force: true });


    const routesForDeleteGuard = await adminFetch('/admin/routes');
    const usedBackendId = routesForDeleteGuard.flatMap((r) => r.backend_ids || [])[0];
    if (usedBackendId) {
      const usedBackend = (await adminFetch('/admin/backends')).find((b) => b.id === usedBackendId);
      if (usedBackend) {
        const usedRow = row(page, usedBackend.name);
        await usedRow.waitFor({ state: 'visible', timeout: 10000 });
        const deleteButton = usedRow.getByRole('button', { name: 'Delete' });
        const disabled = await deleteButton.isDisabled().catch(() => false);
        const state = await deleteButton.evaluate((b) => ({
          disabled: b.disabled,
          ariaDisabled: b.getAttribute('aria-disabled'),
          title: b.getAttribute('title'),
          className: b.className,
        })).catch(() => ({}));
        if (disabled || state.ariaDisabled === 'true') {
          pass('providers', 'Delete is disabled for provider used by routes', { provider: usedBackend.name, id: usedBackend.id, state });
        } else {
          fail('providers', 'in-use Provider Delete is enabled in UI', { provider: usedBackend.name, id: usedBackend.id, state }, 'Disable Delete for providers referenced by routes, or show Used by N routes with links before any destructive click.');
        }
      }
    }

    // Full Provider CRUD through the Portal UI. This catches regressions where Edit creates a new row or Delete only fails after click.
    await page.getByRole('button', { name: /Pre-register provider|Add provider/ }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    await modalField(page, 'Name').fill(`${prefix}-provider`);
    await modalField(page, 'Base URL').fill('http://127.0.0.1:65534/v1');
    await ensureProviderLoadControlOpen(page);
    await modalField(page, 'Weight').fill('1');
    await modalField(page, 'Simultaneous calls').fill('0');
    await modalField(page, 'Provider').selectOption('openai').catch(async () => {
      await modalField(page, 'Provider').selectOption({ label: /OpenAI/i }).catch(() => {});
    });
    await page.locator('.modal').getByRole('button', { name: /Pre-register|Add|Create|Save/ }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'hidden', timeout: 10000 }).catch(() => {});
    await row(page, `${prefix}-provider`).waitFor({ state: 'visible', timeout: 10000 });
    backends = await adminFetch('/admin/backends');
    const uiCreated = backends.find((b) => b.name === `${prefix}-provider`);
    uiProviderId = uiCreated?.id || null;
    if (uiCreated) pass('providers', 'Provider create through UI persists exactly one backend', { id: uiProviderId, name: uiCreated.name });
    else fail('providers', 'Provider create through UI did not persist backend', { names: backends.map((b) => b.name).filter((n) => (n || '').startsWith(prefix)) }, 'Add Provider must POST exactly one backend and refresh table.');

    await row(page, `${prefix}-provider`).getByRole('button', { name: 'Edit' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    await modalField(page, 'Name').fill(`${prefix}-provider-edited`);
    await ensureProviderLoadControlOpen(page);
    await modalField(page, 'Weight').fill('7');
    await modalField(page, 'Simultaneous calls').fill('3');
    await page.locator('.modal').getByRole('button', { name: 'Save' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'hidden', timeout: 10000 }).catch(() => {});
    await row(page, `${prefix}-provider-edited`).waitFor({ state: 'visible', timeout: 10000 });
    const providerRow = await row(page, `${prefix}-provider-edited`).innerText();
    backends = await adminFetch('/admin/backends');
    const editedProvider = backends.find((b) => b.id === uiProviderId);
    const duplicateEdited = backends.filter((b) => b.name === `${prefix}-provider-edited`).length;
    if (/\b7\b/.test(providerRow) && /\b3\b/.test(providerRow) && editedProvider?.weight === 7 && editedProvider?.max_inflight === 3 && duplicateEdited === 1) pass('providers', 'Provider edit updates same row and persists weight/max', { row: providerRow, api: editedProvider });
    else fail('providers', 'Provider edit did not update the same row cleanly', { row: providerRow, api: editedProvider, duplicateEdited }, 'Edit Provider must PATCH the existing backend id, not create a second row.');

    await page.waitForTimeout(1800); // row flash animation after edit must finish before a stable click.
    {
      const b = page.locator('tr').filter({ hasText: `${prefix}-provider-edited` }).getByRole('button', { name: 'Delete' });
      const info = await b.evaluate(el => { const r=el.getBoundingClientRect(); const at=document.elementFromPoint(r.x+r.width/2, r.y+r.height/2); return { disabled: el.disabled, x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height), coveredBy: at ? (at.tagName+'.'+String(at.className).slice(0,40)) : 'none', hasOnclick: !!el.onclick }; }).catch(e => ({err: e.message}));
      console.log('DELETEBTN_STATE', JSON.stringify(info));
    }

    try {
      await row(page, `${prefix}-provider-edited`).getByRole('button', { name: 'Delete' }).click({ timeout: 8000 });
      await page.waitForTimeout(1500);
    } catch (e) {
      fail('providers', 'unused Provider Delete button is not reliably clickable', { error: e.stack || e.message }, 'Provider Delete must be clickable after edit/save without layout animation blocking the action.');
    }
    await row(page, `${prefix}-provider-edited`).waitFor({ state: 'hidden', timeout: 10000 }).catch(() => {});
    backends = await adminFetch('/admin/backends');
    const stillThere = backends.find((b) => b.id === uiProviderId || b.name === `${prefix}-provider-edited`);
    if (!stillThere) { pass('providers', 'unused Provider delete through UI removes backend', { id: uiProviderId }); uiProviderId = null; }
    else fail('providers', 'unused Provider delete through UI did not remove backend', { api: stillThere }, 'Delete unused Provider must remove the backend and refresh the table.');

    const created = await adminFetch('/admin/backends', 'POST', { name: `${prefix}-route-provider`, base_url: 'http://127.0.0.1:65534/v1', api_key_ref: 'env:NONE', weight: 1, max_inflight: 0, format: 'openai', enabled: true });
    providerId = created.id;
    await page.evaluate(() => refresh());
    await nav(page, 'providers');
    await row(page, `${prefix}-route-provider`).waitFor({ state: 'visible', timeout: 10000 });

    await nav(page, 'models');
    await page.getByRole('button', { name: 'Create model route' }).click({ force: true });
    await page.locator('.modal').waitFor({ state: 'visible', timeout: 8000 });
    let beforeOptions = await providerOptions(page);
    result.evidence.routeProviderOptionsInitial = beforeOptions;
    if (beforeOptions.length === 1 && beforeOptions[0].text === 'No providers') fail('models', 'Route wizard initially says No providers even though templates exist', { beforeOptions }, 'Show templates/disabled by default when active list is empty.');
    if (beforeOptions.some((o) => isExternalTestName(o.text))) fail('models', 'Route wizard initial provider list includes external test rows', { beforeOptions }, 'Acceptance must clean stale test data before claiming done.');

    const showDisabled = page.locator('.modal label').filter({ hasText: 'Show disabled' }).locator('input[type="checkbox"]').first();
    if (await showDisabled.count()) { await showDisabled.check({ force: true }); await page.waitForTimeout(400); }
    const allOptions = await providerOptions(page);
    result.evidence.routeProviderOptionsAll = allOptions;
    const names = allOptions.map((o) => o.text.replace(/\s+\(id.*$/i, ''));
    const counts = {};
    for (const n of names) counts[n] = (counts[n] || 0) + 1;
    const dupes = Object.entries(counts).filter(([n, c]) => c > 1 && n !== 'No providers');
    if (dupes.length) fail('models', 'Route provider dropdown has duplicate display names', { dupes, allOptions }, 'Deduplicate seeded providers and make option labels unambiguous.');
    else pass('models', 'route provider dropdown has no duplicate display names', { count: allOptions.length });
    if (allOptions.some((o) => isExternalTestName(o.text))) fail('models', 'Route provider dropdown includes external test provider rows', { allOptions }, 'Clean stale test data before done.');

    const openaiOption = allOptions.find((o) => /^openai\b/i.test(o.text));
    if (openaiOption) {
      await modalField(page, 'Provider').selectOption(openaiOption.value);
      await modalField(page, 'Provider API protocol').selectOption('openai_chat').catch(() => {});
      await modalField(page, 'Authentication').selectOption('bearer').catch(() => {});
      await page.locator('#toast').evaluate((e) => { e.innerHTML = ''; }).catch(() => {});
      await page.locator('.modal').getByRole('button', { name: 'Load models' }).click({ force: true });
      const t = await toastText(page, 1800);
      result.evidence.routeBlankCloudKeyToast = t;
      if (/401|unauthorized|502/i.test(t)) fail('models', 'blank-key cloud Load models leaks provider unauthorized', { toast: t }, 'Validate locally: bearer/anthropic requires key before preview request.');
      else if (/key|enter|required/i.test(t)) pass('models', 'blank-key cloud Load models blocked with friendly validation', { toast: t });
      else fail('models', 'blank-key cloud Load models message unclear', { toast: t }, 'Show Enter API key first.');
    } else {
      fail('models', 'OpenAI template missing from route provider choices', { allOptions }, 'Fresh install must include OpenAI template.');
    }
    await page.locator('.modal').getByRole('button', { name: 'Cancel' }).click({ force: true }).catch(() => {});

    await nav(page, 'models');
    await page.getByRole('button', { name: 'Create model route' }).click({ force: true });
    await modalField(page, 'Provider').selectOption(String(providerId));
    await modalField(page, 'Provider model').fill('qwen3.8-flash-next');
    await modalField(page, 'Public model name').fill(routeName);
    await modalField(page, 'Context window').fill('8192');
    await modalField(page, 'Max output tokens').fill('64');
    await modalField(page, 'Price per 1M input').fill('0.10');
    await modalField(page, 'Price per 1M output').fill('0.20');
    await page.locator('.modal').getByRole('button', { name: 'Save route' }).click({ force: true });
    await row(page, routeName).waitFor({ state: 'visible', timeout: 10000 });
    const routeRow = await row(page, routeName).innerText();
    const route = (await adminFetch('/admin/routes')).find((r) => r.model_name === routeName);
    if (/qwen3\.8-flash-next/.test(routeRow) && /8\.2K|8192/.test(routeRow) && /64/.test(routeRow) && /0\.1/.test(routeRow) && route?.provider_model_name === 'qwen3.8-flash-next') pass('models', 'route production fields visible and persisted', { row: routeRow, api: route });
    else fail('models', 'route production fields hidden/not persisted', { row: routeRow, api: route }, 'Show provider model/auth/context/max output/prices.');

    const team = await adminFetch('/admin/teams', 'POST', { name: `${prefix}-team`, budget: { period: 'month', max_tokens: 1000000, per_model: {} }, enabled: true });
    teamId = team.id;
    const patchedTeam = await adminFetch(`/admin/teams/${teamId}`, 'PATCH', { name: `${prefix}-team-edited`, budget: null, enabled: true });
    const teamsAfter = await adminFetch('/admin/teams');
    const trow = teamsAfter.find((t) => t.id === teamId);
    if (trow && trow.budget === null) pass('teams', 'explicit budget:null clears team budget', { api: trow, patchResponse: patchedTeam });
    else fail('teams', 'explicit budget:null does not clear team budget', { api: trow, patchResponse: patchedTeam }, 'PATCH /admin/teams/{id} must distinguish omitted budget from explicit null and set DB budget=NULL.');

    await nav(page, 'keys');
    await page.getByRole('button', { name: 'New key' }).click({ force: true });
    await modalField(page, 'Team').selectOption('1');
    await modalField(page, 'Owner').fill(`${prefix}-owner`);
    await page.locator('.modal').getByRole('button', { name: 'Restrict to selected models' }).click({ force: true });
    await page.locator(`.modal .scope-chips input[value="${routeName}"]`).click({ force: true });
    await modalField(page, 'Requests per minute').fill('10');
    await modalField(page, 'Simultaneous request limit').fill('2');
    await page.locator('.modal').getByRole('button', { name: 'Create' }).click({ force: true });
    await page.locator('.modal').filter({ hasText: 'Key created' }).waitFor({ state: 'visible', timeout: 10000 });
    const keyText = await page.locator('.modal').innerText();
    const keyMatch = keyText.match(/lc-[A-Za-z0-9._-]+/);
    const createdKey = keyMatch ? keyMatch[0] : null;
    await page.locator('.modal').getByRole('button', { name: 'Done' }).click({ force: true });
    await row(page, `${prefix}-owner`).waitFor({ state: 'visible', timeout: 10000 });
    let key = (await adminFetch('/admin/keys')).find((k) => k.owner === `${prefix}-owner`);
    keyId = key?.id;
    if (createdKey) {
      await page.waitForFunction(({ owner, key }) => {
        return [...document.querySelectorAll('tr')].some((tr) => tr.textContent.includes(owner) && tr.textContent.includes(key));
      }, { owner: `${prefix}-owner`, key: createdKey }, { timeout: 7000 }).catch(() => {});
    }
    const keyRow = await row(page, `${prefix}-owner`).innerText();
    if (createdKey && keyRow.includes(createdKey) && /Edit/.test(keyRow)) pass('keys', 'key reveal and Edit action visible', { row: keyRow.replace(createdKey, redact(createdKey)) });
    else fail('keys', 'key reveal/Edit action missing', { row: createdKey ? keyRow.replace(createdKey, redact(createdKey)) : keyRow }, 'Admin must see revealable keys after async reveal finishes, plus Edit action.');
    if (keyId) {
      await row(page, `${prefix}-owner`).getByRole('button', { name: 'Edit' }).click({ force: true });
      await modalField(page, 'Owner').fill(`${prefix}-owner-edited`);
      await modalField(page, 'Requests per minute').fill('11');
      await modalField(page, 'Simultaneous request limit').fill('3');
      await page.locator('.modal').getByRole('button', { name: 'Save' }).click({ force: true });
      await row(page, `${prefix}-owner-edited`).waitFor({ state: 'visible', timeout: 10000 });
      key = (await adminFetch('/admin/keys')).find((k) => k.id === keyId);
      if (key?.owner === `${prefix}-owner-edited` && key?.rpm_limit === 11 && key?.concurrency_limit === 3) pass('keys', 'API key edit persists metadata', { api: key });
      else fail('keys', 'API key edit did not persist metadata', { api: key }, 'PATCH /admin/keys/{id} must update metadata without regenerating secret.');
    }


    if (process.env.BRIGHTO_SKIP_ROUTE_SMOKE !== '1') {
      const routesToSmoke = (await adminFetch('/admin/routes')).filter((r) => r.enabled !== false && !(r.model_name || '').startsWith(prefix));
      const keysForSmoke = (await adminFetch('/admin/keys')).filter((k) => k.enabled && k.revealable !== false);
      for (const r of routesToSmoke) {
        const keyMeta = keysForSmoke.find((k) => !(k.allowed_models || []).length || (k.allowed_models || []).includes(r.model_name));
        if (!keyMeta) {
          fail('routes', 'enabled route has no enabled revealable client key for smoke test', { model: r.model_name }, 'Do not enable a route unless it has a usable test client key or create one during install.');
          continue;
        }
        const revealed = await adminFetch(`/admin/keys/${keyMeta.id}/reveal`);
        const ctrl = new AbortController();
        const timer = setTimeout(() => ctrl.abort(), Number(process.env.BRIGHTO_ROUTE_SMOKE_TIMEOUT_MS || 45000));
        let smoke;
        try {
          const resp = await fetch(BASE + '/v1/chat/completions', {
            method: 'POST',
            headers: { 'content-type': 'application/json', authorization: `Bearer ${revealed.key}` },
            body: JSON.stringify({ model: r.model_name, messages: [{ role: 'user', content: 'Reply with exactly OK.' }], max_tokens: 8, stream: false }),
            signal: ctrl.signal,
          });
          const text = await resp.text();
          smoke = { status: resp.status, body: text.slice(0, 240), key_prefix: keyMeta.prefix };
        } catch (e) {
          smoke = { error: e.name === 'AbortError' ? 'timeout' : (e.stack || e.message), key_prefix: keyMeta.prefix };
        } finally {
          clearTimeout(timer);
        }
        if (smoke.status === 200) pass('routes', `enabled route smoke works: ${r.model_name}`, { model: r.model_name, key_prefix: keyMeta.prefix });
        else fail('routes', `enabled route smoke failed: ${r.model_name}`, { model: r.model_name, route: r, smoke }, 'Enabled routes must be smoke-tested through the router before being marked enabled/done. Save draft is available without endpoint test and remains disabled; Save enabled requires endpoint test.');
      }
    }

    // User portal smoke with an existing enabled revealable key, preferably not this disposable key.
    const existingKeys = await adminFetch('/admin/keys');
    const userKeyMeta = existingKeys.find((k) => k.enabled && k.revealable !== false && k.id !== keyId) || existingKeys.find((k) => k.enabled && k.revealable !== false);
    if (userKeyMeta) {
      const reveal = await adminFetch(`/admin/keys/${userKeyMeta.id}/reveal`);
      await page.getByRole('button', { name: 'Sign out' }).click({ force: true });
      await page.locator('#seg-user').click({ force: true });
      await page.locator('#login-apikey').fill(reveal.key);
      await page.getByRole('button', { name: 'Sign in' }).click({ force: true });
      await page.locator('#app-view:not(.hidden)').waitFor({ state: 'visible', timeout: 10000 });
      const userState = await page.evaluate(() => ({ providers: getComputedStyle(document.querySelector('#nav-providers')).display, models: getComputedStyle(document.querySelector('#nav-models')).display, keys: getComputedStyle(document.querySelector('#nav-keys')).display, title: document.querySelector('#page-title')?.textContent }));
      if (userState.providers === 'none' && userState.models === 'none' && userState.keys === 'none' && userState.title === 'Dashboard') pass('user', 'user login hides admin menus and lands on Dashboard', userState);
      else fail('user', 'user login state/title is wrong', userState, 'Hide admin-only menus and reset active view/title to Dashboard on user login.');
    }

    if (result.consoleErrors.length) fail('runtime', 'browser console errors observed', { consoleErrors: result.consoleErrors }, 'Normal admin/user flows should have no console errors.');
  } catch (e) {
    fail('audit', 'portal logic acceptance crashed', { error: e.stack || e.message }, 'Fix the crashed flow and rerun this gate.');
  } finally {
    try { await adminFetch(`/admin/routes/${encodeURIComponent(routeName)}`, 'DELETE'); result.cleanup.push('route'); } catch {}
    if (providerId) { try { await adminFetch(`/admin/backends/${providerId}`, 'DELETE'); result.cleanup.push('route provider'); } catch (e) { result.cleanup.push(`route provider failed: ${e.message}`); } }
    if (uiProviderId) { try { await adminFetch(`/admin/backends/${uiProviderId}`, 'DELETE'); result.cleanup.push('ui provider'); } catch (e) { result.cleanup.push(`ui provider failed: ${e.message}`); } }
    if (keyId) { try { await adminFetch(`/admin/keys/${keyId}`, 'DELETE'); result.cleanup.push('key disabled'); } catch {} }
    await browser.close().catch(() => {});
  }
}

await main();
result.result = result.failures.length ? 'FAIL' : 'PASS';
await writeFile(`${OUT}/summary.json`, JSON.stringify(result, null, 2));
console.log(JSON.stringify(result, null, 2));
process.exit(result.result === 'PASS' ? 0 : 1);
