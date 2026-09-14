// SPDX-License-Identifier: AGPL-3.0-only
import { useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import type { ActivityResponse } from "../../../lib/tauri";
import { useActivity } from "../../../lib/useActivity";
import {
  ACTIVITY_RAIL_MIN_WIDTH,
  useActivityNowLayout,
  type ActivityNowLayout,
} from "../../../lib/activityNowLayout";
import {
  assetSentence,
  knownAssets,
  knownLane,
  knownState,
  laneKey,
  routeFor,
  routeSentence,
  ROUTE_JOBS,
  stepCount,
  suggestionLines,
  trustSentence,
  type KnownActivityAsset,
  type KnownActivityAssetKind,
  type KnownActivityStep,
} from "../../../lib/activitySentence";
import { ASSET_ORDER, BlockedCauses } from "./ActivitySummaryPopover";

/**
 * Tier 2: the Now section on the Activity page.
 *
 * Same three assets as the popover, one level deeper: each row expands into
 * its steps, and the section carries the models line and the trust sentence
 * that the popover only summarizes. The feed below is untouched.
 */

const RAIL_QUERY = `(min-width: ${ACTIVITY_RAIL_MIN_WIDTH}px)`;

function subscribeToRailWidth(onChange: () => void): () => void {
  if (typeof window.matchMedia !== "function") return () => {};
  const media = window.matchMedia(RAIL_QUERY);
  media.addEventListener("change", onChange);
  return () => media.removeEventListener("change", onChange);
}

function railWidthSnapshot(): boolean {
  return (
    typeof window.matchMedia === "function" &&
    window.matchMedia(RAIL_QUERY).matches
  );
}

/**
 * The layout actually rendered. The rail has nowhere to sit on a narrow
 * window, so it becomes the card; the user's preference is untouched and comes
 * back when the window widens.
 */
export function effectiveLayout(
  preference: ActivityNowLayout,
  wideEnoughForRail: boolean,
): ActivityNowLayout {
  return preference === "rail" && !wideEnoughForRail ? "card" : preference;
}

/**
 * The effective layout, for callers that must place the section rather than
 * render it. ActivityFeed needs this to decide between a two-column shell, a
 * card above the toolbar, and a group inside the feed.
 */
export function useActivityNowPlacement(): ActivityNowLayout {
  const [preference] = useActivityNowLayout();
  const wideEnoughForRail = useSyncExternalStore(
    subscribeToRailWidth,
    railWidthSnapshot,
    () => true,
  );
  return effectiveLayout(preference, wideEnoughForRail);
}

const ASSET_COLOR: Record<KnownActivityAssetKind, string> = {
  memories: "var(--mem-accent-indigo)",
  entities: "var(--mem-accent-sage)",
  pages: "var(--mem-accent-warm)",
};

function StepRow({
  activity,
  kind,
  step,
}: {
  readonly activity: ActivityResponse;
  readonly kind: KnownActivityAssetKind;
  readonly step: KnownActivityStep;
}) {
  const { t } = useTranslation();
  // Store and Confirm run no model, so they carry no lane. `job` is the
  // server's own answer to "which route does this step use"; a null one means
  // the question does not apply, not that the lane is missing. A lane this
  // build cannot name gets no chip rather than a raw key.
  const lane = step.job === null ? undefined : knownLane(routeFor(activity, kind).lane);
  const count = stepCount(step);

  return (
    <div
      data-testid={`activity-step-${step.name}`}
      style={{
        alignItems: "baseline",
        display: "flex",
        gap: "8px",
        paddingLeft: "16px",
      }}
    >
      <span
        style={{
          color: "var(--mem-text-secondary)",
          fontFamily: "var(--mem-font-body)",
          fontSize: "11px",
          fontWeight: 500,
          minWidth: "64px",
        }}
      >
        {t(`activityStatus.step.${step.name}`)}
      </span>
      <span
        style={{
          color: "var(--mem-text-tertiary)",
          flex: 1,
          fontFamily: "var(--mem-font-body)",
          fontSize: "11px",
          lineHeight: 1.45,
          minWidth: 0,
        }}
      >
        {t(`activityStatus.stepValue.${step.name}`)}
      </span>
      <span
        data-testid={`activity-step-count-${step.name}`}
        style={{
          color: "var(--mem-text-tertiary)",
          fontFamily: "var(--mem-font-mono)",
          fontSize: "10px",
          whiteSpace: "nowrap",
        }}
      >
        {t(count.key, count.params)}
      </span>
      {lane !== undefined && (
        <span
          className="mem-activity-lane-chip"
          data-testid={`activity-step-lane-${step.name}`}
        >
          {t(laneKey(lane))}
        </span>
      )}
    </div>
  );
}

function AssetBlock({
  activity,
  asset,
}: {
  readonly activity: ActivityResponse;
  readonly asset: KnownActivityAsset;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const phrase = assetSentence(activity, asset);

  return (
    <div data-testid={`activity-now-${asset.kind}`} style={{ display: "grid", gap: "5px" }}>
      <div style={{ alignItems: "center", display: "flex", gap: "7px" }}>
        <span
          aria-hidden="true"
          style={{
            backgroundColor: ASSET_COLOR[asset.kind],
            borderRadius: "2px",
            flexShrink: 0,
            height: "8px",
            width: "8px",
          }}
        />
        <span
          style={{
            color: "var(--mem-text)",
            fontFamily: "var(--mem-font-body)",
            fontSize: "12px",
            fontWeight: 500,
          }}
        >
          {t(`activityStatus.asset.${asset.kind}`)}
        </span>
        {asset.steps.length > 0 && (
          <button
            type="button"
            className="mem-activity-steps-toggle"
            data-testid={`activity-steps-toggle-${asset.kind}`}
            aria-expanded={open}
            onClick={() => setOpen((value) => !value)}
          >
            {t(open ? "activityStatus.hideSteps" : "activityStatus.showSteps")}
          </button>
        )}
      </div>
      <p
        style={{
          color: "var(--mem-text-secondary)",
          fontFamily: "var(--mem-font-body)",
          fontSize: "11px",
          lineHeight: 1.45,
          margin: 0,
          paddingLeft: "15px",
        }}
      >
        {t(phrase.key, phrase.params)}
      </p>
      {open && (
        <div
          data-testid={`activity-steps-${asset.kind}`}
          style={{ display: "grid", gap: "5px", paddingTop: "3px" }}
        >
          {asset.steps.map((step) => (
            <StepRow
              key={step.name}
              activity={activity}
              kind={asset.kind}
              step={step}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Open refinement suggestions, after the assets. Quiet on purpose: a neutral
 * square, no state word, because these counts never change the state and
 * some of them never move on their own.
 */
function SuggestionsBlock({ activity }: { readonly activity: ActivityResponse }) {
  const { t } = useTranslation();
  const lines = suggestionLines(activity);
  if (lines.length === 0) return null;

  return (
    <div data-testid="activity-now-suggestions" style={{ display: "grid", gap: "5px" }}>
      <div style={{ alignItems: "center", display: "flex", gap: "7px" }}>
        <span
          aria-hidden="true"
          style={{
            backgroundColor: "var(--mem-text-tertiary)",
            borderRadius: "2px",
            flexShrink: 0,
            height: "8px",
            width: "8px",
          }}
        />
        <span
          style={{
            color: "var(--mem-text)",
            fontFamily: "var(--mem-font-body)",
            fontSize: "12px",
            fontWeight: 500,
          }}
        >
          {t("activityStatus.suggestions.title")}
        </span>
      </div>
      {lines.map((line) => (
        <p
          key={line.kind}
          data-testid={`activity-now-suggestions-${line.kind}`}
          style={{
            color: "var(--mem-text-secondary)",
            fontFamily: "var(--mem-font-body)",
            fontSize: "11px",
            lineHeight: 1.45,
            margin: 0,
            paddingLeft: "15px",
          }}
        >
          {t(line.key, line.params)}
        </p>
      ))}
    </div>
  );
}

function NowBody({
  activity,
  onOpenIntelligence,
}: {
  readonly activity: ActivityResponse;
  readonly onOpenIntelligence?: () => void;
}) {
  const { t } = useTranslation();
  const byKind = new Map(knownAssets(activity).map((asset) => [asset.kind, asset]));
  const trust = trustSentence(activity);
  const state = knownState(activity);
  const trustText =
    trust === undefined
      ? undefined
      : trust.kind === "local"
      ? t(trust.key)
      : t(trust.key, {
          jobs: trust.jobKeys
            .map((key) => t(key))
            .join(t("activityStatus.jobSeparator")),
          vendor: t(trust.vendorKey),
        });

  return (
    <div style={{ display: "grid", gap: "12px" }}>
      {state !== undefined && (
        <p
          data-testid="activity-now-headline"
          style={{
            color: "var(--mem-text)",
            fontFamily: "var(--mem-font-body)",
            fontSize: "12px",
            lineHeight: 1.45,
            margin: 0,
          }}
        >
          {t(`activityStatus.headline.${state}`)}
        </p>
      )}
      <BlockedCauses activity={activity} testId="activity-now-causes" />

      <div style={{ display: "grid", gap: "11px" }}>
        {ASSET_ORDER.map((kind) => {
          const asset = byKind.get(kind);
          return asset ? (
            <AssetBlock key={kind} activity={activity} asset={asset} />
          ) : null;
        })}
      </div>

      <SuggestionsBlock activity={activity} />

      <div
        style={{
          borderTop: "1px solid var(--mem-border)",
          display: "grid",
          gap: "5px",
          paddingTop: "10px",
        }}
      >
        <p
          data-testid="activity-now-models"
          style={{
            color: "var(--mem-text-tertiary)",
            fontFamily: "var(--mem-font-body)",
            fontSize: "10px",
            lineHeight: 1.45,
            margin: 0,
          }}
        >
          {/* A route on a lane this build cannot name is left out: its
              sentence would need a word for where the work runs. */}
          {ROUTE_JOBS.flatMap((job) => {
            const route = activity[job];
            const lane = knownLane(route.lane);
            if (lane === undefined) return [];
            const phrase = routeSentence(route);
            return [
              t(phrase.key, {
                ...phrase.params,
                job: t(`activityStatus.jobTitle.${job}`),
                lane: t(laneKey(lane)),
              }),
            ];
          }).join(" ")}
          {onOpenIntelligence !== undefined && (
            <>
              {" "}
              <button
                type="button"
                className="mem-activity-inline-link"
                data-testid="activity-now-intelligence"
                onClick={onOpenIntelligence}
              >
                {t("activityStatus.openIntelligence")}
              </button>
            </>
          )}
        </p>
        {/* No trust line at all when a lane is unknown: see trustSentence. */}
        {trustText !== undefined && (
          <p
            data-testid="activity-now-trust"
            style={{
              color: "var(--mem-text-tertiary)",
              fontFamily: "var(--mem-font-body)",
              fontSize: "10px",
              lineHeight: 1.45,
              margin: 0,
            }}
          >
            {trustText}
          </p>
        )}
      </div>
    </div>
  );
}

interface ActivityNowProps {
  /** Navigates to Settings, Intelligence. Absent when the shell has no route. */
  readonly onOpenIntelligence?: () => void;
}

/**
 * The Now section, in the layout the user chose in Settings.
 *
 * `rail` and `card` render the same body in a bordered surface; `timeline`
 * drops the surface and uses the feed's own section grammar so it reads as the
 * newest group rather than as a widget parked in the list.
 */
export default function ActivityNow({ onOpenIntelligence }: ActivityNowProps) {
  const { t } = useTranslation();
  const { data: activity } = useActivity();
  const layout = useActivityNowPlacement();

  // Nothing until the first read lands, for the same reason as the status
  // line: an invented headline is worse than an empty slot for one tick.
  if (activity === undefined) return null;

  const title = (
    <h3 className="mem-activity-now-title">{t("activityStatus.nowTitle")}</h3>
  );

  if (layout === "timeline") {
    return (
      <section data-testid="activity-now" data-layout="timeline">
        {title}
        <NowBody activity={activity} onOpenIntelligence={onOpenIntelligence} />
      </section>
    );
  }

  return (
    <section
      className="mem-activity-now-surface"
      data-testid="activity-now"
      data-layout={layout}
    >
      {title}
      <NowBody activity={activity} onOpenIntelligence={onOpenIntelligence} />
    </section>
  );
}
