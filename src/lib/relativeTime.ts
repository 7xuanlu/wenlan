// SPDX-License-Identifier: AGPL-3.0-only
//
// One relative-time vocabulary for every surface that prints "when".
// Extracted from ActivityFeed.tsx unchanged: the activity feed and the
// background-activity surfaces sit on the same page, so a second phrasing of
// "3h ago" beside the first would read as two different clocks.

import type { TFunction } from "i18next";

/** Seconds-since-epoch to "just now" / "12m ago" / a locale date. */
export function relativeTime(
  ts: number,
  t: TFunction,
  language: string,
): string {
  const now = Date.now() / 1000;
  const diff = now - ts;
  if (diff < 60) return t("activity.relative.justNow");
  if (diff < 3600)
    return t("activity.relative.minutesAgo", { count: Math.floor(diff / 60) });
  if (diff < 86400)
    return t("activity.relative.hoursAgo", { count: Math.floor(diff / 3600) });
  if (diff < 604800)
    return t("activity.relative.daysAgo", { count: Math.floor(diff / 86400) });
  return new Date(ts * 1000).toLocaleDateString(language);
}
