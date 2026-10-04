// SPDX-License-Identifier: Apache-2.0
export const pairingJS = `
const main = document.querySelector('main');
const notice = document.querySelector('#notice');
const continueButton = document.querySelector('#continue');
let pollingTimer;
let stopped = false;
let submitting = false;
let statusRequest;
const pollingDeadline = Date.now() + 5 * 60 * 1000;
function stopPolling() {
  stopped = true;
  clearTimeout(pollingTimer);
  statusRequest?.abort();
}
function renderState(result) {
  const status = result.status;
  if (!['pending', 'approved', 'unavailable'].includes(status)) throw new Error('Invalid pairing status');
  main.dataset.pairingState = status;
  const banner = document.querySelector('.status');
  banner.dataset.state = status;
  const label = document.querySelector('#pairing-status') || banner;
  label.textContent = status === 'approved' ? 'Approved on your device'
    : status === 'pending' ? 'Waiting for device approval' : 'This pairing is no longer available.';
  continueButton.disabled = status !== 'approved' || submitting;
  document.querySelector('.pairing-step').hidden = status !== 'pending';
  const space = document.querySelector('#approved-space');
  space.hidden = status !== 'approved';
  space.querySelector('dd').textContent = typeof result.space === 'string' ? result.space : '';
  if (status === 'approved') notice.textContent = 'Ready. Continue to your AI client.';
  if (status === 'pending') notice.textContent = 'This page updates after you approve in Wenlan.';
  if (status === 'unavailable') {
    stopPolling();
    notice.textContent = 'Return to your AI client and start a new connection.';
    document.querySelector('.permissions').hidden = true;
    document.querySelector('.client-details').hidden = true;
    document.querySelector('.actions').hidden = true;
    for (const button of document.querySelectorAll('.actions button')) button.disabled = true;
    document.querySelector('#copy-code').disabled = true;
    document.querySelector('.sample-link')?.remove();
  }
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
      continueButton.disabled = true;
      notice.textContent = 'Unable to check approval. Reconnecting...';
    }
  } finally {
    clearTimeout(timeout);
    if (!stopped && !submitting) {
      if (Date.now() >= pollingDeadline) {
        stopPolling();
        continueButton.disabled = true;
        notice.textContent = 'Approval check ended. Refresh this page to check again.';
      } else pollingTimer = setTimeout(pollStatus, 3000);
    }
  }
}
window.addEventListener('pagehide', stopPolling);
document.querySelector('#copy-code')?.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(document.querySelector('#pairing-code').value);
    document.querySelector('#copy-code').textContent = 'Copied';
    notice.textContent = 'Code copied. Paste it in Wenlan to review this connection.';
  } catch { notice.textContent = 'Select the pairing code and copy it.'; }
});
for (const form of document.querySelectorAll('form')) form.addEventListener('submit', async event => {
  event.preventDefault();
  const button = form.querySelector('button');
  if (submitting || button.disabled) return;
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
    const response = await fetch(form.action, { method: 'POST', credentials: 'same-origin', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body), signal: AbortSignal.timeout(15000) });
    const result = await response.json();
    if (result.redirectTo) { navigated = true; stopPolling(); location.assign(result.redirectTo); }
    else if (result.cancelled) { navigated = true; stopPolling(); location.replace('/pairing'); }
    else if (result.pairingUnavailable === true && continueButton) renderState({ status: 'unavailable' });
    else notice.textContent = result.error || 'Unable to continue.';
  } catch { notice.textContent = 'Connection unavailable. Try again.'; }
  finally {
    const password = form.querySelector('input[type=password]');
    if (password) password.value = '';
    submitting = false;
    if (!navigated && main.dataset.pairingState !== 'unavailable') {
      for (const action of document.querySelectorAll('form button')) action.disabled = false;
      if (continueButton) {
        continueButton.disabled = true;
        pollingTimer = setTimeout(pollStatus, 3000);
      }
    }
  }
});
if (continueButton) pollStatus();
`;
