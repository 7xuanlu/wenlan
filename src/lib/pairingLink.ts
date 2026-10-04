// SPDX-License-Identifier: AGPL-3.0-only
/**
 * A pairing code that arrived through a `wenlan://pair?code=` link from the
 * relay pairing page. Main pulls it from the native side and parks it here;
 * the Connections panel takes it once and opens the review for it.
 */
import { useSyncExternalStore } from "react";

let pending: string | null = null;
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
  publish(code);
}

export function clearPendingPairingCode() {
  publish(null);
}

export function usePendingPairingCode(): string | null {
  return useSyncExternalStore(subscribe, () => pending, () => null);
}
