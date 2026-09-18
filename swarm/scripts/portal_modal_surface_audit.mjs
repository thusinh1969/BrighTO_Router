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
    const apiKeyField = [...modal.querySelectorAll('.field')].find((f) => /^API key$/i.test((f.querySelector('label')?.textContent || '').trim()));
    const apiKeyInput = apiKeyField?.querySelector('input');
    const apiKeyToggle = apiKeyField ? [...apiKeyField.querySelectorAll('button')].find((b) => /^(Show|Hide)$/i.test((b.innerText || b.textContent || '').trim())) : null;
    const apiKeyVisibility = apiKeyField ? {
      inputType: apiKeyInput ? apiKeyInput.type : '',
      hasToggle: !!apiKeyToggle,
      toggleText: apiKeyToggle ? (apiKeyToggle.innerText || apiKeyToggle.textContent || '').trim() : '',
      toggleAria: apiKeyToggle ? (apiKeyToggle.getAttribute('aria-label') || '') : '',
    } : null;
    const modelPickerRows = [...modal.querySelectorAll('.model-picker-row')].map((node) => {
      const r = node.getBoundingClientRect();
      const st = getComputedStyle(node);
      const input = node.querySelector('input');
      const button = node.querySelector('button');
      const ir = input ? input.getBoundingClientRect() : null;
      const br = button ? button.getBoundingClientRect() : null;
      return {
        display: st.display,
        gridTemplateColumns: st.gridTemplateColumns,
        width: Math.round(r.width),
        inputWidth: ir ? Math.round(ir.width) : 0,
        inputBottom: ir ? Math.round(ir.bottom) : 0,
        buttonWidth: br ? Math.round(br.width) : 0,
        buttonTop: br ? Math.round(br.top) : 0,
        buttonText: button ? (button.innerText || button.textContent || '').trim() : '',
      };
    });
    const footerButtons = footer ? [...footer.querySelectorAll('button')].map((node) => {
      const r = node.getBoundingClientRect();
      const st = getComputedStyle(node);
      return { text: (node.innerText || node.textContent || '').trim(), width: Math.round(r.width), height: Math.round(r.height), whiteSpace: st.whiteSpace, scrollWidth: node.scrollWidth, clientWidth: node.clientWidth, scrollHeight: node.scrollHeight, clientHeight: node.clientHeight };
    }) : [];
    const wizardSteps = [...modal.querySelectorAll('.wizard-step')].map((node) => {
      const r = node.getBoundingClientRect();
      return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height), text: (node.innerText || '').trim().replace(/\s+/g, ' ') };
    });
    const gateCopyRects = [...modal.querySelectorAll('.connection-status')].filter((node) => /Run Test connection before saving an enabled route|Model chosen .* Test connection/i.test(node.innerText || node.textContent || '')).map((node) => {
      const r = node.getBoundingClientRect();
      return { top: Math.round(r.top), bottom: Math.round(r.bottom), height: Math.round(r.height), text: (node.innerText || node.textContent || '').trim().replace(/\s+/g, ' ') };
    });
    const footerCoveredInputs = footerRect
      ? inputs.filter((i) => i.bottom > footerRect.top + 4 && i.top < footerRect.bottom - 4)
      : [];
    const disabledPrimaryButtons = [...modal.querySelectorAll('button.btn.primary:disabled')].filter((node) => node.offsetParent !== null).map((node) => {
      const r = node.getBoundingClientRect();
      const style = getComputedStyle(node);
      return {
        text: (node.innerText || node.textContent || '').trim(),
        width: Math.round(r.width),
        backgroundColor: style.backgroundColor,
        borderColor: style.borderColor,
        color: style.color,
        opacity: style.opacity,
      };
    });
    const footerCoveredImportant = footerRect
      ? [...modal.querySelectorAll('.model-map, .advanced-toggle')].filter((node) => node.offsetParent !== null).map((node) => {
          const r = node.getBoundingClientRect();
          const overlap = Math.max(0, Math.min(r.bottom, footerRect.bottom) - Math.max(r.top, footerRect.top));
          return {
            cls: String(node.className),
            text: (node.innerText || node.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 160),
            top: Math.round(r.top),
            bottom: Math.round(r.bottom),
            footerTop: Math.round(footerRect.top),
            footerBottom: Math.round(footerRect.bottom),
            overlap: Math.round(overlap),
          };
        }).filter((x) => x.overlap > 3)
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
      apiKeyVisibility,
      modelPickerRows,
      footerButtons,
      wizardSteps,
      gateCopyRects,
      switches,
      disabledPrimaryButtons,
      footerCoveredImportant,
      text: modal.innerText.slice(0, 2400),
      clipped: clipped.slice(0, 25),
    };
  });
}

async function verifyProviderApiKeyToggle(page, name) {
  const before = await page.evaluate(() => {
    const modal = document.querySelector('#modal-overlay .modal');
    const field = [...modal.querySelectorAll('.field')].find((f) => /^API key$/i.test((f.querySelector('label')?.textContent || '').trim()));
    const input = field?.querySelector('input');
    const button = field ? [...field.querySelectorAll('button')].find((b) => /^(Show|Hide)$/i.test((b.innerText || b.textContent || '').trim())) : null;
    if (input) { input.value = 'sk-visible-check'; input.dispatchEvent(new Event('input', { bubbles: true })); }
    return { inputType: input ? input.type : '', hasButton: !!button, buttonText: button ? (button.innerText || button.textContent || '').trim() : '', aria: button ? (button.getAttribute('aria-label') || '') : '' };
  });
  result.metrics[`${name}-api-key-toggle-before`] = before;
  if (before.inputType !== 'password' || !before.hasButton || before.buttonText !== 'Show' || !/Show provider API key/i.test(before.aria || '')) {
    fail(`${name}: Add model API key field should default hidden with a clear Show action`, before);
    return;
  }
  await page.getByRole('button', { name: 'Show provider API key' }).click({ force: true });
  await page.waitForTimeout(80);
  const shown = await page.evaluate(() => {
    const modal = document.querySelector('#modal-overlay .modal');
    const field = [...modal.querySelectorAll('.field')].find((f) => /^API key$/i.test((f.querySelector('label')?.textContent || '').trim()));
    const input = field?.querySelector('input');
    const button = field ? [...field.querySelectorAll('button')].find((b) => /^(Show|Hide)$/i.test((b.innerText || b.textContent || '').trim())) : null;
    return { inputType: input ? input.type : '', value: input ? input.value : '', buttonText: button ? (button.innerText || button.textContent || '').trim() : '', aria: button ? (button.getAttribute('aria-label') || '') : '' };
  });
  result.metrics[`${name}-api-key-toggle-shown`] = shown;
  if (shown.inputType !== 'text' || shown.value !== 'sk-visible-check' || shown.buttonText !== 'Hide' || !/Hide provider API key/i.test(shown.aria || '')) {
    fail(`${name}: Show provider API key should reveal the typed upstream key`, shown);
  }
  await page.getByRole('button', { name: 'Hide provider API key' }).click({ force: true });
  await page.waitForTimeout(80);
  const hidden = await page.evaluate(() => {
    const modal = document.querySelector('#modal-overlay .modal');
    const field = [...modal.querySelectorAll('.field')].find((f) => /^API key$/i.test((f.querySelector('label')?.textContent || '').trim()));
    const input = field?.querySelector('input');
    const button = field ? [...field.querySelectorAll('button')].find((b) => /^(Show|Hide)$/i.test((b.innerText || b.textContent || '').trim())) : null;
    return { inputType: input ? input.type : '', value: input ? input.value : '', buttonText: button ? (button.innerText || button.textContent || '').trim() : '', aria: button ? (button.getAttribute('aria-label') || '') : '' };
  });
  result.metrics[`${name}-api-key-toggle-hidden`] = hidden;
  if (hidden.inputType !== 'password' || hidden.value !== 'sk-visible-check' || hidden.buttonText !== 'Show' || !/Show provider API key/i.test(hidden.aria || '')) {
    fail(`${name}: Hide provider API key should hide the typed upstream key without clearing it`, hidden);
  }
}


async function verifyModelPickerPreview(page, name) {
  const picked = `${name}-picker-model`;
  await page.route('**/admin/routes/preview-models', async (route) => {
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ models: [picked] }) });
  });
  try {
    await page.evaluate(() => {
      const modal = document.querySelector('#modal-overlay .modal');
      const provider = [...modal.querySelectorAll('select')].find((sel) => [...sel.options].some((o) => /Custom LLM/i.test(o.textContent || '')));
      const opt = provider ? [...provider.options].find((o) => /Custom LLM/i.test(o.textContent || '') && !o.disabled) : null;
      if (provider && opt) {
        provider.value = opt.value;
        provider.dispatchEvent(new Event('change', { bubbles: true }));
      }
      const field = [...modal.querySelectorAll('.field')].find((f) => /^Base URL$/i.test((f.querySelector('label')?.textContent || '').trim()));
      const input = field?.querySelector('input');
      if (input) {
        input.value = 'http://127.0.0.1:9000/v1';
        input.dispatchEvent(new Event('input', { bubbles: true }));
      }
    });
    await page.getByRole('button', { name: /^Load models$/ }).click({ force: true });
    await page.locator('.picker-overlay .picker-item').filter({ hasText: picked }).click({ force: true });
    await page.locator('.picker-overlay').getByRole('button', { name: 'Use this model' }).click({ force: true });
    await page.waitForTimeout(150);
    const state = await page.evaluate(() => {
      const modal = document.querySelector('#modal-overlay .modal');
      function fieldValue(label) {
        const field = [...modal.querySelectorAll('.field')].find((f) => (f.querySelector('label')?.textContent || '').trim().toLowerCase() === label.toLowerCase());
        const input = field?.querySelector('input,select,textarea');
        return input ? input.value : '';
      }
      return {
        providerModel: fieldValue('Provider model'),
        publicModel: fieldValue('Public model name (shown to clients)'),
        mapText: modal.querySelector('.model-map')?.innerText || '',
        statusText: [...modal.querySelectorAll('.connection-status')].map((n) => n.innerText || n.textContent || '').join('\n'),
        pickerStillOpen: !!document.querySelector('.picker-overlay'),
      };
    });
    result.metrics[`${name}-picker-preview`] = state;
    if (state.pickerStillOpen || state.providerModel !== picked || state.publicModel !== picked || !state.mapText.includes(picked) || !/Model chosen .* Test connection/i.test(state.statusText || '')) {
      fail(`${name}: choosing a loaded provider model must update inputs, mapping preview, and Save-enabled gate`, state);
    }
  } finally {
    await page.unroute('**/admin/routes/preview-models').catch(() => {});
  }
}


async function verifyRouteAdvancedDrawerAutoscroll(page, name) {
  await page.locator('.modal').evaluate((node) => { node.scrollTop = 75; });
  await page.waitForTimeout(120);
  await page.getByRole('button', { name: /Optional limits and pricing/ }).click({ force: true });
  await page.waitForTimeout(500);
  const state = await page.evaluate(() => {
    const modal = document.querySelector('#modal-overlay .modal');
    const footer = modal?.querySelector('.actions');
    const panel = modal?.querySelector('.advanced-panel:not(.hidden)');
    const firstField = panel?.querySelector('.field');
    function rect(node) {
      if (!node) return null;
      const r = node.getBoundingClientRect();
      return { top: Math.round(r.top), bottom: Math.round(r.bottom), height: Math.round(r.height) };
    }
    return {
      scrollTop: Math.round(modal?.scrollTop || 0),
      footer: rect(footer),
      panel: rect(panel),
      firstField: rect(firstField),
      text: (modal?.innerText || '').slice(0, 2200),
    };
  });
  result.metrics[`${name}-advanced-autoscroll`] = state;
  if (!/Context window \(tokens, optional\)/i.test(state.text || '') || !state.firstField || !state.footer || state.firstField.bottom > state.footer.top - 4) {
    fail(`${name}: opening Optional limits must scroll first advanced field above sticky actions`, state);
  }
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
  if (name.endsWith('add-model') && metrics.footerCoveredImportant?.length) fail(`${name}: action footer covers important Add model controls`, metrics);
  const activeLookingDisabledPrimary = (metrics.disabledPrimaryButtons || []).filter((b) => /Save enabled|Use this model|Sign in/i.test(b.text || '') && (/rgb\(29, 78, 216\)|rgb\(30, 64, 175\)/.test(b.backgroundColor || '') || /rgb\(29, 78, 216\)|rgb\(30, 64, 175\)/.test(b.borderColor || '')));
  if (activeLookingDisabledPrimary.length) fail(`${name}: disabled primary buttons still look active`, { activeLookingDisabledPrimary, metrics });
  if (name.startsWith('mobile-')) {
    if (metrics.rect && metrics.rect.bottom > metrics.clientH + 3) fail(`${name}: mobile modal extends below viewport instead of scrolling internally`, metrics);
    if (metrics.modalScrollHeight <= metrics.modalClientHeight && name.endsWith('add-model') && metrics.rect.height > metrics.clientH - 30) fail(`${name}: long mobile modal should scroll internally`, metrics);
  }
  if (name.endsWith('add-model')) {
    if (!/Exact upstream model name returned by the provider/i.test(metrics.text || '')) fail(`${name}: Add model modal missing Provider model help text`, metrics);
    if (!/model name your apps send/i.test(metrics.text || '')) fail(`${name}: Add model modal missing Public model help text`, metrics);
    if (!/Client sends|Provider receives/i.test(metrics.text || '')) fail(`${name}: Add model modal missing public-to-provider model mapping preview`, metrics);
    if (/public-model|provider-model/i.test(metrics.text || '')) fail(`${name}: Add model mapping preview uses fake technical placeholder values`, metrics);
    if (!/Public name|Provider model/i.test(metrics.text || '')) fail(`${name}: Add model mapping preview should clearly show empty state before a model is chosen`, metrics);
    if (!/Gemini \(coming soon\)|Meta Muse \(coming soon\)/i.test(metrics.text || '')) fail(`${name}: Add model provider picker must mark coming-soon providers`, metrics);
    if (name.startsWith('desktop-') && metrics.footer && metrics.footer.bottom > metrics.clientH + 3) fail(`${name}: Add model primary actions must be visible on desktop`, metrics);
    await verifyModelPickerPreview(page, name);
    if (!/Optional limits and pricing|fallback provider/i.test(metrics.text || '')) fail(`${name}: Add model modal missing optional limits/pricing drawer`, metrics);
    if (/Fallback backend/i.test(metrics.text || '')) fail(`${name}: Add model modal exposes backend jargon`, metrics);
    if (!/Save draft/i.test(metrics.text || '')) fail(`${name}: Add model modal must expose a disabled draft save action`, metrics);
    if (!/Run Test connection before saving an enabled route/i.test(metrics.text || '')) fail(`${name}: Add model modal must explain the Save enabled gate`, metrics);
    if (/Save disabled/i.test(metrics.text || '')) fail(`${name}: Add model modal exposes technical Save disabled wording`, metrics);
    if (!metrics.apiKeyVisibility || metrics.apiKeyVisibility.inputType !== 'password' || !metrics.apiKeyVisibility.hasToggle || !/Show provider API key/i.test(metrics.apiKeyVisibility.toggleAria || '')) {
      fail(`${name}: Add model modal should let admins show/hide the upstream API key they type`, metrics);
    }
    await verifyProviderApiKeyToggle(page, name);
    if (name.startsWith('mobile-')) {
      const steps = metrics.wizardSteps || [];
      const firstRow = steps.filter((r) => steps[0] && Math.abs(r.y - steps[0].y) <= 4);
      if (steps.length !== 3 || firstRow.length !== 3) fail(`${name}: Add model wizard steps must stay compact on one mobile row`, metrics);
      const cramped = steps.filter((r) => r.width < 90 || r.height > 72);
      if (cramped.length) fail(`${name}: Add model wizard step chips are cramped on mobile`, { cramped, metrics });
      if (!metrics.footer || metrics.footer.top < 0 || metrics.footer.bottom > metrics.clientH + 3) {
        fail(`${name}: Add model Test/Save actions must stay visible on mobile`, metrics);
      }
      const visibleGateCopy = (metrics.gateCopyRects || []).some((r) => r.top >= 0 && r.bottom <= (metrics.footer ? metrics.footer.top - 4 : metrics.clientH));
      if (!visibleGateCopy) fail(`${name}: Add model Save enabled gate copy must be visible above sticky actions on mobile`, metrics);
      const crampedPickerRows = (metrics.modelPickerRows || []).filter((r) => r.display !== 'grid' || r.inputWidth < 260 || r.buttonWidth < r.inputWidth - 4 || r.buttonTop <= r.inputBottom);
      if (crampedPickerRows.length || !(metrics.modelPickerRows || []).length) fail(`${name}: Add model Provider model picker should stack cleanly on mobile`, { crampedPickerRows, metrics });
      const crampedFooterButtons = (metrics.footerButtons || []).filter((b) => b.scrollWidth > b.clientWidth + 4 || b.scrollHeight > b.clientHeight + 4);
      if (crampedFooterButtons.length) fail(`${name}: Add model footer actions should use short readable mobile labels without clipping`, { crampedFooterButtons, metrics });
      await verifyRouteAdvancedDrawerAutoscroll(page, name);
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
