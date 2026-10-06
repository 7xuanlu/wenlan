// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef } from "react";
import type { KeyboardEvent } from "react";
import { Pulse, WarningCircle } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { knownState, type KnownActivityState } from "../../../lib/activitySentence";
import { useActivity } from "../../../lib/useActivity";
import ActivitySummaryPopover from "./ActivitySummaryPopover";

/**
 * Tier 0 of the activity surfaces: the Activity button in the window toolbar.
 *
 * It is the ONE way into activity. The toolbar sits on every view (Settings
 * and a collapsed sidebar included), so the state rides on the button that
 * already names the destination rather than on a second entry in the sidebar.
 * The icon carries the state (see ActivityIcon), so there is no separate dot.
 * No number: memories, entities and pages are
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

  // Until the first read lands the button is the plain Activity button: an
  // icon claiming a state before the app has asked would be a claim it cannot back.
  // It is the same element either way, so focus survives the first read. A
  // state from a daemon newer than this app claims nothing either: the icon
  // stays plain, but the summary still opens, since its counts still hold.
  // Gating the summary on a known state would unmount it under an open flag
  // the parent never clears, and a later poll would reopen it unasked.
  const loaded = activity !== undefined;
  const state = activity === undefined ? undefined : knownState(activity);
  const stateWord = state === undefined ? undefined : t(`activityStatus.state.${state}`);
  const buttonLabel = stateWord === undefined ? label : t("activityStatus.buttonLabel", { state: stateWord });
  const quiet = state === undefined || state === "up_to_date" || state === "off";
  // Only active work animates; waiting and AI-off stay still.
  const running = state === "organizing";

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
        data-state={state}
        aria-current={current ? "page" : undefined}
        aria-haspopup={loaded ? "dialog" : undefined}
        aria-expanded={loaded ? expanded : undefined}
        aria-label={buttonLabel}
        title={buttonLabel}
        onClick={loaded ? onToggle : onOpenActivity}
        className="mem-activity-status"
      >
        <ActivityIcon state={quiet ? undefined : state} running={running} />
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

/** A pulse denotes activity; a distinct warning shape is reserved for a real block. */
function ActivityIcon({ state, running }: {
  readonly state?: KnownActivityState;
  readonly running: boolean;
}) {
  const Icon = state === "blocked" ? WarningCircle : Pulse;
  return (
    <Icon
      aria-hidden="true"
      className={`mem-activity-status-icon${running ? " mem-activity-pulse-running" : ""}`}
      data-testid="activity-status-icon"
      data-icon-state={state}
      data-icon-kind={state === "blocked" ? "attention" : "pulse"}
      size={18}
      weight="regular"
    />
  );
}
