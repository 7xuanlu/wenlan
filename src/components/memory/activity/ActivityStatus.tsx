// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef } from "react";
import type { KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { useActivity } from "../../../lib/useActivity";
import ActivitySummaryPopover from "./ActivitySummaryPopover";

/**
 * Tier 0 of the activity surfaces: the Activity button in the window toolbar.
 *
 * It is the ONE way into activity. The toolbar sits on every view (Settings
 * and a collapsed sidebar included), so the state rides on the button that
 * already names the destination rather than on a second entry in the sidebar.
 * Up to date looks like the plain button; Steeping adds a pulsing ring;
 * Blocked adds a solid amber dot. No number: memories, entities and pages are
 * counted in different units, so any one figure here would be wrong for two of
 * them. The counts live in the summary, each beside its own unit. Amber is not
 * red on purpose: blocked work is waiting, nothing is lost.
 * A click opens the summary below it; the summary links to the full page.
 */

interface ActivityStatusProps {
  readonly expanded: boolean;
  readonly onToggle: () => void;
  /** Navigates to the Activity view. Absent when the shell has no such route. */
  readonly onOpenActivity?: () => void;
  /** Navigates to Settings, Intelligence, where a missing model is chosen. */
  readonly onOpenIntelligence?: () => void;
  /** True while the Activity view is the current page. */
  readonly current?: boolean;
}

export default function ActivityStatus({
  expanded,
  onToggle,
  onOpenActivity,
  onOpenIntelligence,
  current = false,
}: ActivityStatusProps) {
  const { t } = useTranslation();
  const { data: activity } = useActivity();
  // The trigger ref lives here because Escape has to put focus back on the
  // exact element that opened the popover.
  const triggerRef = useRef<HTMLButtonElement>(null);
  const anchorRef = useRef<HTMLDivElement>(null);

  const close = () => {
    onToggle();
    triggerRef.current?.focus();
  };

  // A dropdown closes when the user clicks anywhere else. Focus is left where
  // the click put it, so this does not steal it back to the trigger.
  useEffect(() => {
    if (!expanded) return;
    const closeOnOutside = (event: MouseEvent) => {
      if (anchorRef.current?.contains(event.target as Node)) return;
      onToggle();
    };
    document.addEventListener("mousedown", closeOnOutside);
    return () => document.removeEventListener("mousedown", closeOnOutside);
  }, [expanded, onToggle]);

  // Escape lives on the anchor, not on the popover: after a click focus is
  // still on the trigger, which is the popover's SIBLING, so a handler on the
  // popover alone would never see the key.
  const closeOnEscape = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Escape" || !expanded) return;
    event.preventDefault();
    event.stopPropagation();
    close();
  };

  const label = t("main.activity");

  // Until the first read lands the button is the plain Activity button: a dot
  // claiming a state before the app has asked would be a claim it cannot back.
  // It is the same element either way, so focus survives the first read.
  const loaded = activity !== undefined;
  const stateWord = loaded ? t(`activityStatus.state.${activity.state}`) : undefined;
  const quiet = !loaded || activity.state === "up_to_date";

  return (
    <div
      ref={anchorRef}
      className="mem-activity-status-anchor"
      onKeyDown={closeOnEscape}
    >
      <button
        ref={triggerRef}
        type="button"
        data-testid="activity-status"
        data-state={activity?.state}
        aria-current={current ? "page" : undefined}
        aria-haspopup={loaded ? "dialog" : undefined}
        aria-expanded={loaded ? expanded : undefined}
        aria-label={loaded ? t("activityStatus.buttonLabel", { state: stateWord }) : undefined}
        title={stateWord}
        onClick={loaded ? onToggle : onOpenActivity}
        className="mem-activity-status"
      >
        <ActivityIcon />
        <span>{label}</span>
        {!quiet && (
          <span className="mem-activity-status-badge">
            <span
              aria-hidden="true"
              data-testid="activity-status-dot"
              data-dot-state={activity.state}
              className={
                activity.state === "organizing"
                  ? "mem-activity-dot mem-activity-dot-pulse"
                  : "mem-activity-dot"
              }
            />
          </span>
        )}
      </button>
      {loaded && expanded && (
        <ActivitySummaryPopover
          activity={activity}
          onClose={close}
          onOpenActivity={onOpenActivity}
          onOpenIntelligence={onOpenIntelligence}
        />
      )}
    </div>
  );
}

function ActivityIcon() {
  return (
    <svg aria-hidden="true" fill="none" height="16" viewBox="0 0 24 24" width="16">
      <path
        d="M3 12a9 9 0 109-9 9.75 9.75 0 00-6.74 2.74L3 8M3 3v5h5M12 7v5l3 2"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.8"
      />
    </svg>
  );
}
