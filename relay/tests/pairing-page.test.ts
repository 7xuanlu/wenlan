// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { authorizeFailureDocument, copy as rawCopy, htmlEscape, negotiateLocale, pairingDocument, returnDocument, spaceLabel,
  type PageLocale, type PairingPageView } from '../src/pairing-page.ts';
import { pairingJS } from '../src/pairing-script.ts';

/** Copy as it appears in markup. */
const copy = (...args: Parameters<typeof rawCopy>) => htmlEscape(rawCopy(...args));
const locales: PageLocale[] = ['en', 'zh-Hant', 'zh-Hans'];
const pairingId = 'a'.repeat(64);
const pending: PairingPageView = { pairingId, clientId: 'client-123', status: 'pending', userCode: 'ABCD2345',
  redirectHost: 'claude.ai', knownClient: true, displayName: 'Claude' };

test('Accept-Language maps to en, zh-Hant or zh-Hans', () => {
  for (const [header, locale] of [
    [null, 'en'], ['', 'en'], ['en-US,en;q=0.9', 'en'], ['fr-FR,de;q=0.8', 'en'],
    ['zh-TW,zh;q=0.9', 'zh-Hant'], ['zh-HK', 'zh-Hant'], ['zh-MO', 'zh-Hant'], ['zh-Hant', 'zh-Hant'],
    ['zh-Hant-CN', 'zh-Hant'], ['zh-hant-tw', 'zh-Hant'],
    ['zh-CN', 'zh-Hans'], ['zh', 'zh-Hans'], ['zh-SG', 'zh-Hans'], ['zh-Hans-HK', 'zh-Hans'],
    ['fr-FR,zh-TW;q=0.8,en;q=0.5', 'zh-Hant'], ['en;q=0.4,zh-CN;q=0.9', 'zh-Hans'],
    ['zh-TW;q=0,en', 'en'], ['ja,ko;q=0.9', 'en'], ['*', 'en'],
  ] as const) assert.equal(negotiateLocale(header), locale, String(header));
});

test('every locale defines the same keys and none uses an em dash', () => {
  for (const locale of locales) {
    for (const key of ['docTitle', 'title', 'lead', 'allow', 'allowHint', 'waiting', 'approved', 'denied', 'expired',
      'fallback', 'fallbackBody', 'touchBody', 'sendsTo', 'unknown', 'fallbackName', 'codeLabel', 'space', 'wholeLibrary', 'continue',
      'cancel', 'cancelled', 'details', 'clientId', 'retry', 'error', 'noscript', 'sample', 'privacy', 'terms',
      'failedTitle', 'failedBody', 'localHint'] as const) {
      const value = rawCopy(locale, key, { name: 'Claude', host: 'claude.ai' });
      assert(value && !value.includes('{'), `${locale}.${key}`);
      assert(!value.includes('—'), `${locale}.${key} has an em dash`);
    }
  }
  assert.equal(copy('en', 'title', { name: 'Claude' }), 'Connect Claude to Wenlan');
  assert.equal(copy('zh-Hant', 'title', { name: 'Claude' }), '將 Claude 連接到 Wenlan');
  assert.equal(copy('zh-Hans', 'title', { name: 'Claude' }), '将 Claude 连接到 Wenlan');
  assert.equal(rawCopy('zh-Hant', 'expired', { name: '你的 AI 應用程式' }), '這個請求已過期，請從你的 AI 應用程式重新開始連接。');
  assert.equal(rawCopy('zh-Hans', 'expired', { name: 'Claude' }), '这个请求已过期，请从 Claude 重新开始连接。');
  assert.equal(rawCopy('en', 'wholeLibrary'), 'Whole library');
  assert.equal(rawCopy('zh-Hant', 'wholeLibrary'), '整個資料庫');
  assert.equal(rawCopy('zh-Hans', 'wholeLibrary'), '整个资料库');
  assert.match(copy('zh-Hant', 'fallbackBody'), /網頁存取/);
  assert.match(copy('zh-Hans', 'fallbackBody'), /网页访问/);
  assert.match(copy('zh-Hans', 'fallbackBody'), /设置/);
  // The local-app hint names the app's own "Add a tool" label in each locale.
  assert.equal(rawCopy('en', 'localHint'),
    "Connecting an app on this computer, like Claude Code or Codex? Use Add a tool in Wenlan instead. That connection doesn't expire.");
  assert.equal(rawCopy('zh-Hant', 'localHint'), '要連接這台電腦上的應用程式（例如 Claude Code 或 Codex）？請改用 Wenlan 中的「新增工具」，這種連接不會過期。');
  assert.equal(rawCopy('zh-Hans', 'localHint'), '要连接这台电脑上的应用（例如 Claude Code 或 Codex）？请改用 Wenlan 中的“添加工具”，这种连接不会过期。');
});

test('the pending page leads with Allow in Wenlan and shows the short code as the fallback', () => {
  for (const locale of locales) {
    const { html, state } = pairingDocument(pending, locale);
    assert.equal(state, 'pending');
    assert.match(html, new RegExp(`<html lang="${locale}">`));
    assert(html.includes(`<h1>${copy(locale, 'title', { name: 'Claude' })}</h1>`), locale);
    assert(html.includes(`href="wenlan://pair?code=${pairingId}">${copy(locale, 'allow')}</a>`), locale);
    assert(html.includes('<output id="user-code">ABCD-2345</output>'), locale);
    assert(html.includes(copy(locale, 'fallback')), locale);
    assert(html.includes(copy(locale, 'touchBody')), locale);
    assert(html.includes(copy(locale, 'waiting')), locale);
    assert(html.includes(copy(locale, 'sendsTo', { host: 'claude.ai' })), locale);
    assert(!html.includes('class="warning"'), 'a known client has no warning');
    assert.match(html, /id="continue"[^>]*hidden disabled/, 'Continue stays in the markup for no-JS');
    assert(!html.includes('<script>'), 'no inline script under the CSP');
    assert(html.includes(`data-text-approved="${copy(locale, 'approved', { name: 'Claude' })}"`));
    const details = html.match(/<details class="client-details">[\s\S]*?<\/details>/)?.[0] ?? '';
    assert(details.includes(`<p class="local-hint">${copy(locale, 'localHint')}</p>`), `${locale}: the local-app hint sits inside Connection details`);
    assert.equal(html.split(copy(locale, 'localHint')).length, 2, `${locale}: the hint appears once`);
  }
});

test('an unknown client shows its redirect host and a warning, never its DCR name', () => {
  for (const locale of locales) {
    const { html } = pairingDocument({ ...pending, redirectHost: 'phish.example', knownClient: false,
      displayName: 'phish.example' }, locale);
    assert(html.includes(`<p class="warning" role="note">${copy(locale, 'unknown')}</p>`), locale);
    assert(html.includes(`<h1>${copy(locale, 'title', { name: 'phish.example' })}</h1>`), locale);
  }
});

test('approved, denied and expired pages render their state copy', () => {
  for (const locale of locales) {
    const approved = pairingDocument({ ...pending, status: 'approved', userCode: undefined, space: 'Work' }, locale);
    assert.equal(approved.state, 'approved');
    assert(approved.html.includes(copy(locale, 'approved', { name: 'Claude' })));
    assert.match(approved.html, /<dl id="approved-space" data-space="Work"><dt>[^<]+<\/dt><dd>Work<\/dd>/);
    assert.doesNotMatch(approved.html, /id="continue"[^>]*disabled/);
    assert.match(approved.html, /<section class="pairing-step" hidden>/);
    const denied = pairingDocument({ ...pending, status: 'denied', userCode: undefined }, locale);
    assert.equal(denied.state, 'denied');
    assert(denied.html.includes(copy(locale, 'denied')));
    const expired = pairingDocument(null, locale);
    assert.equal(expired.state, 'unavailable');
    assert(expired.html.includes(copy(locale, 'expired', { name: copy(locale, 'fallbackName') })));
    assert(!expired.html.includes('wenlan://'));
  }
});

test('the whole-library Space shows a localized label and keeps the raw value for the script', () => {
  for (const locale of locales) {
    const label = copy(locale, 'wholeLibrary');
    const { html } = pairingDocument({ ...pending, status: 'approved', userCode: undefined, space: '*' }, locale);
    assert(html.includes(`<dl id="approved-space" data-space="*"><dt>${copy(locale, 'space')}</dt><dd>${label}</dd></dl>`), locale);
    assert(html.includes(`data-text-whole-library="${label}"`), `${locale}: the script gets the label from <main>`);
    assert.equal(html.split('<dd>*</dd>').length, 1, `${locale}: the raw value is never the visible text`);
    // Not approved yet: the dl stays hidden but still carries the raw value.
    const waiting = pairingDocument({ ...pending, space: '*' }, locale).html;
    assert(waiting.includes('<dl id="approved-space" hidden data-space="*">'), locale);
  }
  assert.equal(spaceLabel('en', '*'), 'Whole library');
  assert.equal(spaceLabel('zh-Hant', '*'), '整個資料庫');
  assert.equal(spaceLabel('zh-Hans', '*'), '整个资料库');
});

test('a named Space renders unchanged and escaped in every locale', () => {
  for (const locale of locales) {
    for (const space of ['Work', '研究 & <b>"x"</b>', '**', ' * ', 'Whole library']) {
      const { html } = pairingDocument({ ...pending, status: 'approved', userCode: undefined, space }, locale);
      const escaped = htmlEscape(space);
      assert(html.includes(`data-space="${escaped}"><dt>`), `${locale}: ${space}`);
      assert(html.includes(`<dd>${escaped}</dd></dl>`), `${locale}: ${space}`);
      assert.equal(spaceLabel(locale, space), space);
    }
    assert(!pairingDocument({ ...pending, status: 'approved', userCode: undefined, space: '<b>x</b>' }, locale).html.includes('<b>x</b>'));
  }
});

test('page values are escaped', () => {
  const { html } = pairingDocument({ ...pending, clientId: '<script>x</script>', redirectHost: 'a"b.example',
    knownClient: false, displayName: '<img src=x>' }, 'en');
  assert(!html.includes('<script>x'));
  assert(!html.includes('<img src=x>'));
  assert(html.includes('&lt;img src=x&gt;'));
  assert(html.includes('a&quot;b.example'));
});

test('authorize failure and no-JS return pages are localized', () => {
  for (const locale of locales) {
    const failure = authorizeFailureDocument(locale);
    assert(failure.includes(copy(locale, 'failedTitle')), locale);
    assert(failure.includes(`lang="${locale}"`));
    const back = returnDocument(locale, 'cancelled', 'https://client.example/cb?error=access_denied&state=s');
    assert(back.includes('href="https://client.example/cb?error=access_denied&amp;state=s"'), locale);
  }
});

// ---- browser script behavior in a minimal DOM ----

interface Node { hidden?: boolean; disabled?: boolean; textContent?: string; dataset: Record<string, string>;
  form?: Form; id?: string; querySelector?: (selector: string) => unknown; remove?: () => void }
interface Form extends Node { action: string; button: Node; listeners: ((event: { preventDefault(): void }) => void)[] }

function harness(initialState: string, responses: Record<string, unknown[]>, start = 0, initialSpace?: { raw: string; shown: string }) {
  let now = start;
  const timers = new Map<number, { at: number; run: () => void }>();
  let timerId = 0;
  const node = (extra: Partial<Node> = {}): Node => ({ dataset: {}, hidden: false, textContent: '', ...extra });
  const main = node({ dataset: { pairingState: initialState, textWaiting: 'WAITING', textApproved: 'APPROVED',
    textDenied: 'DENIED', textExpired: 'EXPIRED', textRetry: 'RETRY', textError: 'ERROR', textWholeLibrary: 'WHOLE' } });
  const status = node();
  const label = node();
  const step = node({ hidden: initialState !== 'pending' });
  const dd = node({ textContent: initialSpace?.shown ?? '' });
  const space = node({ hidden: initialState !== 'approved', dataset: initialSpace ? { space: initialSpace.raw } : {}, querySelector: () => dd });
  const notice = node();
  const form = (action: string): Form => {
    const button = node({ disabled: false });
    const value = { ...node(), action, button, listeners: [], getAttribute: () => action,
      querySelector: (selector: string) => selector === 'button' ? button : null } as Form;
    (value as unknown as { addEventListener: unknown }).addEventListener = (_: string, listener: Form['listeners'][0]) => value.listeners.push(listener);
    button.form = value;
    return value;
  };
  const complete = form('/pairing/complete');
  complete.button.disabled = initialState !== 'approved';
  const cancel = form('/pairing/cancel');
  const all: Record<string, unknown> = { 'main': main, '.status': status, '#pairing-status': label, '#notice': notice,
    '#continue': complete.button, 'form[action="/pairing/cancel"]': cancel, '#approved-space': space,
    '#approved-space dd': dd };
  const calls: { url: string; method: string }[] = [];
  const navigations: string[] = [];
  const document = {
    querySelector: (selector: string) => all[selector] ?? null,
    querySelectorAll: (selector: string) => {
      if (selector === 'form') return [complete, cancel];
      if (selector === 'form button' || selector === '.actions button') return [complete.button, cancel.button];
      if (selector === '.pairing-step') return [step];
      return [];
    },
  };
  const context = {
    document, main,
    window: { addEventListener() {} },
    location: { assign: (url: string) => navigations.push(url), replace: (url: string) => navigations.push(`replace:${url}`) },
    fetch: async (url: string, init: { method?: string } = {}) => {
      calls.push({ url, method: init.method ?? 'GET' });
      const queue = responses[url] ?? [];
      const next = queue.length > 1 ? queue.shift() : queue[0];
      if (next === undefined) throw new Error('offline');
      return { ok: true, json: async () => next };
    },
    FormData: class { get() { return null; } },
    AbortController, AbortSignal: { timeout: () => undefined },
    setTimeout: (run: () => void, delay: number) => { timers.set(++timerId, { at: now + delay, run }); return timerId; },
    clearTimeout: (id: number) => { timers.delete(id); },
    Date: { now: () => now },
    JSON, Error,
  };
  vm.createContext(context);
  vm.runInContext(pairingJS, context);
  const settle = async () => { for (let n = 0; n < 20; n++) await new Promise(resolve => setImmediate(resolve)); };
  const advance = async (ms: number) => {
    const target = now + ms;
    for (;;) {
      const due = [...timers.entries()].filter(([, timer]) => timer.at <= target).sort((a, b) => a[1].at - b[1].at)[0];
      if (!due) break;
      timers.delete(due[0]);
      now = Math.max(now, due[1].at);
      due[1].run();
      await settle();
    }
    now = target;
  };
  return { main, label, step, space, dd, complete, cancel, calls, navigations, settle, advance, pendingTimers: () => timers.size };
}

test('script: approval in Wenlan auto-submits completion once and follows the redirect', async () => {
  const page = harness('pending', {
    '/pairing/status': [{ status: 'pending' }, { status: 'approved', space: 'Work' }],
    '/pairing/complete': [{ redirectTo: 'https://client.example/cb?code=c' }],
  });
  await page.settle();
  assert.equal(page.main.dataset.pairingState, 'pending');
  assert.equal(page.label.textContent, 'WAITING');
  await page.advance(3000);
  assert.equal(page.main.dataset.pairingState, 'approved');
  assert.equal(page.label.textContent, 'APPROVED');
  assert.equal(page.step.hidden, true);
  assert.deepEqual(page.calls.filter(call => call.method === 'POST'), [{ url: '/pairing/complete', method: 'POST' }]);
  assert.deepEqual(page.navigations, ['https://client.example/cb?code=c']);
  await page.advance(60_000);
  assert.equal(page.calls.filter(call => call.method === 'POST').length, 1, 'completion is submitted once');
});

test('script: a server-rendered approved page completes without polling first', async () => {
  const page = harness('approved', { '/pairing/complete': [{ redirectTo: 'https://client.example/cb?code=c' }] });
  await page.settle();
  assert.deepEqual(page.calls, [{ url: '/pairing/complete', method: 'POST' }]);
  assert.deepEqual(page.navigations, ['https://client.example/cb?code=c']);
});

test('script: a whole-library status shows the localized label and keeps the raw value', async () => {
  const page = harness('pending', {
    '/pairing/status': [{ status: 'pending' }, { status: 'approved', space: '*' }],
    '/pairing/complete': [{ redirectTo: 'https://client.example/cb?code=c' }],
  });
  await page.settle();
  await page.advance(3000);
  assert.equal(page.dd.textContent, 'WHOLE');
  assert.equal(page.space.dataset.space, '*');
  assert.equal(page.space.hidden, false);
});

test('script: a named Space status shows the Space itself', async () => {
  const page = harness('pending', {
    '/pairing/status': [{ status: 'approved', space: 'Work' }],
    '/pairing/complete': [{ redirectTo: 'https://client.example/cb?code=c' }],
  });
  await page.settle();
  assert.equal(page.dd.textContent, 'Work');
  assert.equal(page.space.dataset.space, 'Work');
});

test('script: a server-rendered whole-library page reads the raw value back, never the label', async () => {
  for (const locale of locales) {
    const { html } = pairingDocument({ ...pending, status: 'approved', userCode: undefined, space: '*' }, locale);
    const raw = html.match(/<dl id="approved-space"[^>]* data-space="([^"]*)"/)![1];
    assert.equal(raw, '*');
    const page = harness('approved', { '/pairing/complete': [{ redirectTo: 'https://client.example/cb?code=c' }] }, 0,
      { raw, shown: html.match(/<dd>([^<]*)<\/dd><\/dl>/)![1] });
    await page.settle();
    assert.equal(page.space.dataset.space, '*', locale);
    assert.equal(page.dd.textContent, 'WHOLE', `${locale}: the label is re-derived from the raw value`);
    assert.deepEqual(page.navigations, ['https://client.example/cb?code=c']);
  }
});

test('script: a denial auto-submits cancel and returns access_denied to the client', async () => {
  const page = harness('pending', {
    '/pairing/status': [{ status: 'denied' }],
    '/pairing/cancel': [{ cancelled: true, redirectTo: 'https://client.example/cb?error=access_denied' }],
  });
  await page.settle();
  assert.equal(page.label.textContent, 'DENIED');
  assert.deepEqual(page.calls.filter(call => call.method === 'POST'), [{ url: '/pairing/cancel', method: 'POST' }]);
  assert.deepEqual(page.navigations, ['https://client.example/cb?error=access_denied']);
});

test('script: a server-rendered denied page (reload after deny) submits cancel once without polling', async () => {
  const page = harness('denied', {
    '/pairing/cancel': [{ cancelled: true, redirectTo: 'https://client.example/cb?error=access_denied' }],
  });
  await page.settle();
  assert.equal(page.main.dataset.pairingState, 'denied');
  assert.equal(page.label.textContent, 'DENIED');
  assert.deepEqual(page.calls, [{ url: '/pairing/cancel', method: 'POST' }], 'cancel only, no status poll');
  assert.deepEqual(page.navigations, ['https://client.example/cb?error=access_denied']);
  await page.advance(60_000);
  assert.equal(page.calls.filter(call => call.method === 'POST').length, 1, 'cancel is submitted once');
});

test('script: polling stops at the ten-minute deadline and shows the expired copy', async () => {
  const page = harness('pending', { '/pairing/status': [{ status: 'pending' }] }, 1_000_000);
  await page.settle();
  await page.advance(10 * 60 * 1000 - 3001);
  assert.equal(page.main.dataset.pairingState, 'pending');
  const polls = page.calls.length;
  await page.advance(6000);
  assert.equal(page.main.dataset.pairingState, 'unavailable');
  assert.equal(page.label.textContent, 'EXPIRED');
  assert.equal(page.pendingTimers(), 0, 'no further polling');
  await page.advance(60_000);
  assert(page.calls.length <= polls + 2);
  assert.equal(page.calls.filter(call => call.method === 'POST').length, 0);
});

test('script: an unavailable status stops polling without submitting', async () => {
  const page = harness('pending', { '/pairing/status': [{ status: 'unavailable' }] });
  await page.settle();
  assert.equal(page.label.textContent, 'EXPIRED');
  assert.equal(page.pendingTimers(), 0);
  assert.equal(page.calls.filter(call => call.method === 'POST').length, 0);
});
