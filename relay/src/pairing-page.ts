// SPDX-License-Identifier: Apache-2.0
// Browser pairing page markup and copy. Pure functions: no storage or auth.
import { formatUserCode } from './pairing.ts';

export type PageLocale = 'en' | 'zh-Hant' | 'zh-Hans';

const COPY = {
  en: {
    docTitle: 'Connect to Wenlan',
    title: 'Connect {name} to Wenlan',
    lead: '{name} wants to search your Wenlan library.',
    allow: 'Allow in Wenlan',
    allowHint: 'Wenlan opens and asks you to confirm.',
    waiting: 'Waiting for you in Wenlan...',
    approved: 'Allowed. Taking you back to {name}...',
    denied: 'You declined this in Wenlan.',
    expired: 'This request expired. Start connecting again from {name}.',
    fallback: "Wenlan didn't open?",
    fallbackBody: 'In Wenlan, open Settings, then Web access, choose "Have a code?" and enter:',
    touchBody: 'Open Wenlan on your computer. In Settings, then Web access, choose "Have a code?" and enter:',
    sendsTo: 'Sends access to {host}',
    unknown: "Wenlan doesn't recognize this app. Continue only if you started this yourself.",
    fallbackName: 'your AI app',
    codeLabel: 'Your code',
    space: 'Space',
    continue: 'Continue',
    cancel: 'Cancel',
    cancelled: 'Cancelled. Continue to return to your AI app.',
    details: 'Connection details',
    localHint: "Connecting an app on this computer, like Claude Code or Codex? Use Add a tool in Wenlan instead. That connection doesn't expire.",
    clientId: 'Client ID',
    retry: "Can't check for approval right now. Trying again...",
    error: 'Something went wrong. Try again.',
    noscript: 'JavaScript is off. After you allow in Wenlan, reload this page and press Continue.',
    sample: 'Connect a sample library',
    privacy: 'Privacy',
    terms: 'Terms',
    failedTitle: "This connection link didn't work",
    failedBody: 'Go back to your AI app and start connecting to Wenlan again.',
  },
  'zh-Hant': {
    docTitle: '連接到 Wenlan',
    title: '將 {name} 連接到 Wenlan',
    lead: '{name} 想要搜尋你的 Wenlan 資料庫。',
    allow: '在 Wenlan 中允許',
    allowHint: 'Wenlan 會開啟並請你確認。',
    waiting: '正在等待你在 Wenlan 中確認…',
    approved: '已允許，正在返回 {name}…',
    denied: '你已在 Wenlan 中拒絕這個請求。',
    expired: '這個請求已過期，請從 {name} 重新開始連接。',
    fallback: 'Wenlan 沒有開啟？',
    fallbackBody: '在 Wenlan 中開啟「設定」的「網頁存取」，選擇「有代碼嗎？」，然後輸入：',
    touchBody: '在電腦上開啟 Wenlan，到「設定」的「網頁存取」，選擇「有代碼嗎？」，然後輸入：',
    sendsTo: '存取權會交給 {host}',
    unknown: 'Wenlan 無法辨識這個應用程式。只有在你自己發起連接時才繼續。',
    fallbackName: '你的 AI 應用程式',
    codeLabel: '你的代碼',
    space: 'Space',
    continue: '繼續',
    cancel: '取消',
    cancelled: '已取消。按「繼續」返回你的 AI 應用程式。',
    details: '連接詳細資料',
    localHint: '要連接這台電腦上的應用程式（例如 Claude Code 或 Codex）？請改用 Wenlan 中的「新增工具」，這種連接不會過期。',
    clientId: '用戶端 ID',
    retry: '目前無法確認是否已允許，正在重試…',
    error: '發生錯誤，請再試一次。',
    noscript: 'JavaScript 已關閉。在 Wenlan 中允許後，請重新載入此頁面並按「繼續」。',
    sample: '連接範例資料庫',
    privacy: '隱私權',
    terms: '條款',
    failedTitle: '這個連接連結無法使用',
    failedBody: '請返回你的 AI 應用程式，重新開始連接 Wenlan。',
  },
  'zh-Hans': {
    docTitle: '连接到 Wenlan',
    title: '将 {name} 连接到 Wenlan',
    lead: '{name} 想要搜索你的 Wenlan 资料库。',
    allow: '在 Wenlan 中允许',
    allowHint: 'Wenlan 会打开并请你确认。',
    waiting: '正在等待你在 Wenlan 中确认…',
    approved: '已允许，正在返回 {name}…',
    denied: '你已在 Wenlan 中拒绝这个请求。',
    expired: '这个请求已过期，请从 {name} 重新开始连接。',
    fallback: 'Wenlan 没有打开？',
    fallbackBody: '在 Wenlan 中打开“设置”的“网页访问”，选择“有代码吗？”，然后输入：',
    touchBody: '在电脑上打开 Wenlan，到“设置”的“网页访问”，选择“有代码吗？”，然后输入：',
    sendsTo: '访问权限会交给 {host}',
    unknown: 'Wenlan 无法识别这个应用。只有在你自己发起连接时才继续。',
    fallbackName: '你的 AI 应用',
    codeLabel: '你的代码',
    space: 'Space',
    continue: '继续',
    cancel: '取消',
    cancelled: '已取消。按“继续”返回你的 AI 应用。',
    details: '连接详细信息',
    localHint: '要连接这台电脑上的应用（例如 Claude Code 或 Codex）？请改用 Wenlan 中的“添加工具”，这种连接不会过期。',
    clientId: '客户端 ID',
    retry: '暂时无法确认是否已允许，正在重试…',
    error: '出错了，请再试一次。',
    noscript: 'JavaScript 已关闭。在 Wenlan 中允许后，请重新加载此页面并按“继续”。',
    sample: '连接示例资料库',
    privacy: '隐私',
    terms: '条款',
    failedTitle: '这个连接链接无法使用',
    failedBody: '请返回你的 AI 应用，重新开始连接 Wenlan。',
  },
} as const satisfies Record<PageLocale, Record<string, string>>;
export type CopyKey = keyof typeof COPY.en;

export function copy(locale: PageLocale, key: CopyKey, values: Record<string, string> = {}): string {
  const text = COPY[locale][key].replace(/\{(\w+)\}/g, (match, name: string) => values[name] ?? match);
  // The spaces around {name} suit a Latin name; drop them between CJK characters.
  return locale === 'en' ? text : text.replace(/([　-鿿＀-￯]) (?=[　-鿿＀-￯])/g, '$1');
}

/** Highest-weighted supported language from Accept-Language. zh-TW, zh-HK,
 * zh-MO and any Hant script read Traditional; other Chinese reads Simplified.
 */
export function negotiateLocale(header: string | null | undefined): PageLocale {
  if (!header) return 'en';
  const ranges = header.slice(0, 1024).split(',').map((part, index) => {
    const [tag, ...params] = part.trim().split(';');
    const q = params.map(param => /^\s*q\s*=\s*([0-9.]+)\s*$/i.exec(param)?.[1]).find(value => value !== undefined);
    const weight = q === undefined ? 1 : Number(q);
    return { tag: tag.trim().toLowerCase(), weight: Number.isFinite(weight) ? weight : 0, index };
  }).filter(range => range.tag && range.weight > 0)
    .sort((a, b) => b.weight - a.weight || a.index - b.index);
  for (const { tag } of ranges) {
    if (tag === 'en' || tag.startsWith('en-') || tag === '*') return 'en';
    if (tag !== 'zh' && !tag.startsWith('zh-')) continue;
    const subtags = tag.split('-').slice(1);
    if (subtags.includes('hans')) return 'zh-Hans';
    if (subtags.includes('hant') || subtags.some(subtag => ['tw', 'hk', 'mo'].includes(subtag))) return 'zh-Hant';
    return 'zh-Hans';
  }
  return 'en';
}

export function htmlEscape(value: string): string {
  return value.replace(/[&<>"']/g, character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!);
}

export interface PairingPageView {
  pairingId: string;
  clientId: string;
  status: string;
  space?: string;
  userCode?: string;
  redirectHost?: string;
  knownClient?: boolean;
  displayName?: string | null;
}

/** The whole document. Strings the script needs later live in data
 * attributes because the CSP forbids inline script.
 */
export function pairingDocument(view: PairingPageView | null, locale: PageLocale, sampleAvailable = false): { html: string; state: string } {
  const t = (key: CopyKey, values: Record<string, string> = {}) => htmlEscape(copy(locale, key, values));
  const name = view?.displayName || copy(locale, 'fallbackName');
  const state = !view ? 'unavailable'
    : view.status === 'approved' ? 'approved' : view.status === 'denied' ? 'denied' : 'pending';
  const data = (['waiting', 'approved', 'denied', 'expired', 'retry', 'error'] as const)
    .map(key => ` data-text-${key}="${t(key, { name })}"`).join('');
  if (!view) {
    const body = `<h1>${t('docTitle')}</h1>
<p class="status" data-state="unavailable" role="status" aria-live="polite"><span class="status-dot" aria-hidden="true"></span><span id="pairing-status">${t('expired', { name })}</span></p>`;
    return { html: shell(locale, body, state, data), state };
  }
  const pending = state === 'pending';
  const approved = state === 'approved';
  const host = view.redirectHost ?? '';
  const statusKey = approved ? 'approved' : state === 'denied' ? 'denied' : 'waiting';
  const code = view.userCode ? formatUserCode(view.userCode) : '';
  const body = `<h1>${t('title', { name })}</h1>
<p class="intro">${t('lead', { name })}</p>
${view.knownClient === true ? '' : `<p class="warning" role="note">${t('unknown')}</p>`}
<p class="status" data-state="${state}" role="status" aria-live="polite"><span class="status-dot" aria-hidden="true"></span><span id="pairing-status">${t(statusKey, { name })}</span></p>
<section class="pairing-step"${pending ? '' : ' hidden'}>
<div class="open-app"><a id="open-in-wenlan" class="button primary" href="wenlan://pair?code=${htmlEscape(view.pairingId)}">${t('allow')}</a><p class="hint">${t('allowHint')}</p></div>
${code ? `<div class="fallback"><h2 class="desktop-only">${t('fallback')}</h2>
<p><span class="desktop-only">${t('fallbackBody')}</span><span class="touch-only">${t('touchBody')}</span></p>
<p class="user-code"><span class="visually-hidden">${t('codeLabel')}: </span><output id="user-code">${htmlEscape(code)}</output></p></div>` : ''}
</section>
<dl id="approved-space"${approved ? '' : ' hidden'}><dt>${t('space')}</dt><dd>${htmlEscape(view.space ?? '')}</dd></dl>
${host ? `<p class="sends-to">${t('sendsTo', { host })}</p>` : ''}
<details class="client-details"><summary>${t('details')}</summary><dl><dt>${t('clientId')}</dt><dd>${htmlEscape(view.clientId)}</dd></dl><p class="local-hint">${t('localHint')}</p></details>
<p id="notice" role="status" aria-live="polite"></p>
<div class="actions"><form action="/pairing/complete" method="post"><button id="continue" type="submit" class="primary"${approved ? '' : ' hidden disabled'}>${t('continue')}</button></form>
<form action="/pairing/cancel" method="post"><button id="cancel" type="submit">${t('cancel')}</button></form></div>
${sampleAvailable && pending ? `<a class="sample-link" href="/pairing/sample">${t('sample')}</a>` : ''}
<noscript><p>${t('noscript')}</p></noscript>`;
  return { html: shell(locale, body, state, data), state };
}

/** No-JavaScript hand-off back to the client. A link, not a redirect: the
 * page's CSP form-action would block a cross-origin redirect after a post.
 */
export function returnDocument(locale: PageLocale, outcome: 'approved' | 'cancelled', href: string): string {
  const t = (key: CopyKey, values: Record<string, string> = {}) => htmlEscape(copy(locale, key, values));
  const status = outcome === 'approved' ? t('approved', { name: copy(locale, 'fallbackName') }) : t('cancelled');
  return shell(locale, `<h1>${t('docTitle')}</h1>
<p class="status" data-state="${outcome}" role="status"><span class="status-dot" aria-hidden="true"></span><span>${status}</span></p>
<div class="actions"><a id="return-to-client" class="button primary" href="${htmlEscape(href)}">${t('continue')}</a></div>`, 'returning');
}

/** Friendly page for an /authorize request that cannot start pairing. */
export function authorizeFailureDocument(locale: PageLocale): string {
  const t = (key: CopyKey) => htmlEscape(copy(locale, key));
  return shell(locale, `<h1>${t('failedTitle')}</h1>
<p class="status" data-state="unavailable" role="alert"><span class="status-dot" aria-hidden="true"></span><span>${t('failedBody')}</span></p>`, 'failed', '');
}

export function shell(locale: PageLocale, body: string, state: string, data = ''): string {
  const t = (key: CopyKey) => htmlEscape(copy(locale, key));
  return `<!doctype html><html lang="${locale}"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>${t('docTitle')}</title><link rel="icon" href="/icon.png"><link rel="stylesheet" href="/pairing.css"><script src="/pairing.js" defer></script></head>
<body><main data-pairing-state="${htmlEscape(state)}"${data}><header class="brand"><img src="/icon.png" width="32" height="32" alt=""><span>Wenlan</span></header>${body}
<footer><a href="https://wenlan.app/docs/data-and-privacy" rel="noreferrer">${t('privacy')}</a><a href="https://wenlan.app/terms" rel="noreferrer">${t('terms')}</a><a href="https://wenlan.app" rel="noreferrer">Wenlan</a></footer></main></body></html>`;
}
