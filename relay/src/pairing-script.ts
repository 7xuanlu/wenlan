// SPDX-License-Identifier: Apache-2.0
// Browser pairing page behavior. Localized strings come from data attributes
// on <main>, rendered by the server, because the CSP forbids inline script.
export const pairingJS = `
const main = document.querySelector('main');
const text = key => main.dataset['text' + key[0].toUpperCase() + key.slice(1)] || '';
const notice = document.querySelector('#notice');
const continueButton = document.querySelector('#continue');
const completeForm = continueButton ? continueButton.form : null;
const cancelForm = document.querySelector('form[action="/pairing/cancel"]');
let pollingTimer;
let stopped = false;
let submitting = false;
let autoSubmitted = false;
let statusRequest;
const pollingDeadline = Date.now() + 10 * 60 * 1000;
function stopPolling() {
  stopped = true;
  clearTimeout(pollingTimer);
  statusRequest?.abort();
}
function setNotice(value) { if (notice) notice.textContent = value; }
function hide(selector, hidden) { for (const node of document.querySelectorAll(selector)) node.hidden = hidden; }
function renderState(result) {
  const status = result.status;
  if (!['pending', 'approved', 'denied', 'unavailable'].includes(status)) throw new Error('Invalid pairing status');
  main.dataset.pairingState = status;
  const banner = document.querySelector('.status');
  banner.dataset.state = status;
  const label = document.querySelector('#pairing-status') || banner;
  label.textContent = text(status === 'approved' ? 'approved' : status === 'denied' ? 'denied'
    : status === 'pending' ? 'waiting' : 'expired');
  hide('.pairing-step', status !== 'pending');
  const space = document.querySelector('#approved-space');
  if (space) {
    space.hidden = status !== 'approved';
    space.querySelector('dd').textContent = typeof result.space === 'string' ? result.space : '';
  }
  if (continueButton) {
    continueButton.hidden = status !== 'approved';
    continueButton.disabled = status !== 'approved' || submitting;
  }
  if (status === 'pending') setNotice('');
  if (status === 'unavailable') {
    stopPolling();
    setNotice('');
    hide('.intro, .warning, .sends-to, .client-details, .actions, #approved-space', true);
    for (const button of document.querySelectorAll('.actions button')) button.disabled = true;
    document.querySelector('.sample-link')?.remove();
  }
  // Approval in Wenlan is the consent; the browser continues by itself.
  if (!autoSubmitted && !submitting && status === 'approved' && completeForm) { autoSubmitted = true; submit(completeForm); }
  if (!autoSubmitted && !submitting && status === 'denied' && cancelForm) { autoSubmitted = true; submit(cancelForm); }
}
async function pollStatus() {
  if (stopped || submitting) return;
  statusRequest = new AbortController();
  const timeout = setTimeout(() => statusRequest.abort(), 8000);
  try {
    const response = await fetch('/pairing/status', { credentials: 'same-origin', cache: 'no-store', signal: statusRequest.signal });
    if (!response.ok) throw new Error('Status unavailable');
    const result = await response.json();
    if (!stopped && !submitting) renderState(result);
  } catch {
    if (!stopped && !submitting) {
      if (continueButton) continueButton.disabled = true;
      setNotice(text('retry'));
    }
  } finally {
    clearTimeout(timeout);
    if (!stopped && !submitting) {
      if (Date.now() >= pollingDeadline) renderState({ status: 'unavailable' });
      else pollingTimer = setTimeout(pollStatus, 3000);
    }
  }
}
async function submit(form) {
  const button = form.querySelector('button');
  if (submitting || (button && button.disabled && form !== cancelForm)) return;
  submitting = true;
  clearTimeout(pollingTimer);
  statusRequest?.abort();
  for (const action of document.querySelectorAll('form button')) action.disabled = true;
  let navigated = false;
  try {
    const fields = new FormData(form);
    const sample = form.id === 'sample-login';
    const body = sample ? { username: fields.get('username'), password: fields.get('password'), approved: fields.get('approved') === 'on',
      clientId: form.dataset.clientId, resource: form.dataset.resource, space: form.dataset.space } : {};
    const response = await fetch(form.getAttribute('action'), { method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body), signal: AbortSignal.timeout(15000) });
    const result = await response.json();
    if (result.redirectTo) { navigated = true; stopPolling(); location.assign(result.redirectTo); }
    else if (result.cancelled) { navigated = true; stopPolling(); location.replace('/pairing'); }
    else if (result.pairingUnavailable === true && continueButton) renderState({ status: 'unavailable' });
    else setNotice(sample ? (result.error || text('error')) : text('error'));
  } catch { setNotice(text('error') || 'Connection unavailable. Try again.'); }
  finally {
    const password = form.querySelector('input[type=password]');
    if (password) password.value = '';
    submitting = false;
    if (!navigated && main.dataset.pairingState !== 'unavailable') {
      for (const action of document.querySelectorAll('form button')) action.disabled = false;
      if (continueButton) {
        continueButton.disabled = main.dataset.pairingState !== 'approved';
        if (!stopped) pollingTimer = setTimeout(pollStatus, 3000);
      }
    }
  }
}
window.addEventListener('pagehide', stopPolling);
for (const form of document.querySelectorAll('form')) form.addEventListener('submit', event => {
  event.preventDefault();
  submit(form);
});
if (continueButton) {
  const initial = main.dataset.pairingState;
  if (initial === 'approved' || initial === 'denied') renderState({ status: initial,
    space: document.querySelector('#approved-space dd')?.textContent || undefined });
  else pollStatus();
}
`;
