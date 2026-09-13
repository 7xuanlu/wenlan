// SPDX-License-Identifier: AGPL-3.0-only
import { useTranslation } from "react-i18next";
import type { ActivityResponse, ActivityState } from "../../../lib/tauri";
import { useActivity } from "../../../lib/useActivity";

/**
 * Tier 0 of the activity surfaces: one quiet line at the bottom of the
 * sidebar saying whether Wenlan is working on the user's behalf.
 *
 * It is present on every page, so it must stay silent about detail and never
 * grow taller than one row. The number beside the word is the busiest asset's
 * progress when organizing, and the count of stuck items when blocked.
 */

/** Dot color per state. Amber is not red on purpose: nothing is lost when
 *  work is blocked, it is waiting, so the palette must not read as an error. */
const DOT_COLOR: Record<ActivityState, string> = {
  up_to_date: "var(--mem-accent-sage)",
  organizing: "var(--mem-accent-indigo)",
  blocked: "var(--mem-accent-amber)",
};

/**
 * The trailing number.
 *
 * Organizing shows the busiest asset — the one with the most work left, not
 * the first in the list — because that is the one the user is waiting on.
 * Blocked shows the total stuck across every asset.
 */
export function statusDetail(activity: ActivityResponse): string | null {
  if (activity.state === "blocked") {
    const blocked = activity.assets.reduce((sum, a) => sum + a.blocked, 0);
    return blocked > 0 ? String(blocked) : null;
  }
  if (activity.state === "organizing") {
    const busiest = activity.assets
      .filter((a) => a.total > a.done)
      .sort((a, b) => b.total - b.done - (a.total - a.done))[0];
    return busiest ? `${busiest.done}/${busiest.total}` : null;
  }
  return null;
}

interface ActivityStatusProps {
  readonly expanded: boolean;
  readonly onToggle: () => void;
}

export default function ActivityStatus({
  expanded,
  onToggle,
}: ActivityStatusProps) {
  const { t } = useTranslation();
  const { data: activity } = useActivity();

  // Render nothing until the first read lands. A line that says "Up to date"
  // before it has asked would be a claim the app cannot back, and this sits
  // on every page where it would be the first thing a new user reads.
  if (activity === undefined) return null;

  const detail = statusDetail(activity);

  return (
    <button
      type="button"
      data-testid="activity-status"
      data-state={activity.state}
      aria-haspopup="dialog"
      aria-expanded={expanded}
      aria-label={t("activityStatus.statusLabel")}
      onClick={onToggle}
      className="mem-activity-status"
      style={{
        alignItems: "center",
        background: "transparent",
        border: "none",
        borderTop: "1px solid var(--mem-border)",
        color: "var(--mem-text-tertiary)",
        cursor: "pointer",
        display: "flex",
        fontSize: "11px",
        gap: "7px",
        padding: "8px 16px",
        textAlign: "left",
        width: "100%",
      }}
    >
      <span
        aria-hidden="true"
        data-testid="activity-status-dot"
        data-dot-state={activity.state}
        className={
          activity.state === "organizing" ? "mem-activity-dot-pulse" : undefined
        }
        style={{
          backgroundColor: DOT_COLOR[activity.state],
          borderRadius: "50%",
          flexShrink: 0,
          height: "6px",
          width: "6px",
        }}
      />
      <span
        style={{
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {t(`activityStatus.state.${activity.state}`)}
      </span>
      {detail !== null && (
        <span
          data-testid="activity-status-detail"
          style={{
            fontFamily: "var(--mem-font-mono)",
            fontSize: "10px",
            marginLeft: "auto",
          }}
        >
          {detail}
        </span>
      )}
    </button>
  );
}
