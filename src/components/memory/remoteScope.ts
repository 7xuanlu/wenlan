// SPDX-License-Identifier: AGPL-3.0-only
import type { TFunction } from "i18next";
import type { RemoteScope } from "../../lib/tauri";

export type { RemoteScope };

/**
 * The saved scope value for "every Space, plus everything not in a Space".
 * The daemon refuses it as a Space name, so a one-Space choice can never mean it.
 */
export const WHOLE_LIBRARY = "*";

export const wholeLibrary: RemoteScope = { kind: "wholeLibrary" };

/** The scope a saved profile holds, or null when nothing is saved. */
export function savedScope(space: string | null | undefined): RemoteScope | null {
  if (!space) return null;
  return space === WHOLE_LIBRARY ? wholeLibrary : { kind: "space", name: space };
}

export function sameScope(a: RemoteScope | null, b: RemoteScope | null): boolean {
  if (!a || !b || a.kind !== b.kind) return false;
  return a.kind === "wholeLibrary" || a.name === (b as { name: string }).name;
}

/** "Whole library" in the person's language, or the Space name. */
export function scopeLabel(t: TFunction, scope: RemoteScope | string): string {
  const resolved = typeof scope === "string" ? savedScope(scope) : scope;
  if (!resolved) return "";
  return resolved.kind === "wholeLibrary" ? t("remoteAccess.wholeLibrary") : resolved.name;
}
