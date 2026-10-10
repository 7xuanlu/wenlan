// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Two small shared facts about web pairing.
 *
 * 1. A pairing code that arrived through a `wenlan://pair?code=` link from the
 *    relay pairing page, or that someone typed into "Have a code?". Main (or the
 *    panel) parks it here; the approval dialog takes it and closes it again.
 *    While a request is being looked up or decided, a newer code is dropped:
 *    swapping the request under someone about to click Allow could grant a
 *    different browser than the one they were reading about.
 * 2. That a request was just allowed. The AI app finishes connecting a moment
 *    later, and the connected-apps list polls quickly for that short while.
 */
import { useEffect, useReducer, useSyncExternalStore } from "react";

let pending: string | null = null;
let held = false;
const listeners = new Set<() => void>();

function publish(next: string | null) {
  if (pending === next) return;
  pending = next;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

export function setPendingPairingCode(code: string) {
  if (held && pending !== null) return;
  publish(code);
}

export function clearPendingPairingCode() {
  held = false;
  publish(null);
}

/** The dialog holds the open request while it is looked up or decided. */
export function holdPendingPairingCode(hold: boolean) {
  held = hold;
}

export function usePendingPairingCode(): string | null {
  return useSyncExternalStore(subscribe, () => pending, () => null);
}

/** How long after an approval the list keeps looking for the new connection. */
export const AWAIT_CONNECTION_MS = 2 * 60 * 1000;

let approvedAt: number | null = null;
const approvalListeners = new Set<() => void>();

function publishApproval(next: number | null) {
  if (approvedAt === next) return;
  approvedAt = next;
  for (const listener of approvalListeners) listener();
}

function subscribeApproval(listener: () => void) {
  approvalListeners.add(listener);
  return () => { approvalListeners.delete(listener); };
}

export function markPairingApproved(now: number = Date.now()) {
  publishApproval(now);
}

export function clearAwaitingConnection() {
  publishApproval(null);
}

/** When the latest request was allowed, while its connection is still expected. Otherwise null. */
export function useAwaitingConnection(): number | null {
  const at = useSyncExternalStore(subscribeApproval, () => approvedAt, () => null);
  const [, refresh] = useReducer((count: number) => count + 1, 0);
  useEffect(() => {
    if (at === null) return;
    const left = at + AWAIT_CONNECTION_MS - Date.now();
    if (left <= 0) return;
    const timer = window.setTimeout(refresh, left + 1);
    return () => window.clearTimeout(timer);
  }, [at]);
  return at !== null && Date.now() < at + AWAIT_CONNECTION_MS ? at : null;
}
