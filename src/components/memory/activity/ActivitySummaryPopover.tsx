// SPDX-License-Identifier: AGPL-3.0-only
import { useTranslation } from "react-i18next";
import type {
  ActivityAssetKind,
  ActivityAssetStatus,
  ActivityResponse,
} from "../../../lib/tauri";
import { relativeTime } from "../../../lib/relativeTime";
import {
  assetCount,
  assetProgress,
  assetSentence,
  blockedCauses,
  trustSentence,
} from "../../../lib/activitySentence";

/**
 * Tier 1: the summary the toolbar Activity button opens.
 *
 * It opens on what the user can act on: a missing model and the button that
 * fixes it, or, when nothing needs them, one headline. Then the three assets
 * the user already knows from the nav, each counted in its own unit, and a
 * trust line. The steps behind each asset stay one level down on the Activity
 * page: this surface answers "what is Wenlan doing for me", not "how".
 */

/**
 * The order the rows render in, independent of the order the daemon sends.
 * Memories first because everything starts there, then what was found in them,
 * then what was written from them.
 */
export const ASSET_ORDER: readonly ActivityAssetKind[] = [
  "memories",
  "entities",
  "pages",
];

/**
 * Pages is NOT `--mem-accent-page`: in the light theme that token and
 * `--mem-accent-indigo` are the same value, so Memories and Pages would carry
 * identical squares. Warm is the nearest token that stays distinct in both
 * themes.
 */
const ASSET_COLOR: Record<ActivityAssetKind, string> = {
  memories: "var(--mem-accent-indigo)",
  entities: "var(--mem-accent-sage)",
  pages: "var(--mem-accent-warm)",
};

interface ActivitySummaryPopoverProps {
  readonly activity: ActivityResponse;
  /** Closes the popover and returns focus to the Activity button. */
  readonly onClose: () => void;
  /** Navigates to the Activity view. Absent when the shell has no such route. */
  readonly onOpenActivity?: () => void;
  /** Navigates to Settings, Intelligence. Absent when the shell has no route. */
  readonly onOpenIntelligence?: () => void;
}

function AssetRow({
  activity,
  asset,
}: {
  readonly activity: ActivityResponse;
  readonly asset: ActivityAssetStatus;
}) {
  const { t } = useTranslation();
  const phrase = assetSentence(activity, asset);
  const fraction = assetProgress(asset);

  return (
    <div data-testid={`activity-asset-${asset.kind}`} style={{ display: "grid", gap: "4px" }}>
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
        <span
          data-testid={`activity-asset-count-${asset.kind}`}
          style={{
            color: "var(--mem-text-tertiary)",
            fontFamily: "var(--mem-font-mono)",
            fontSize: "10px",
            marginLeft: "auto",
          }}
        >
          {assetCount(asset)}
        </span>
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
      <div
        aria-hidden="true"
        style={{
          backgroundColor: "var(--mem-border)",
          borderRadius: "2px",
          height: "3px",
          marginLeft: "15px",
          overflow: "hidden",
        }}
      >
        <div
          data-testid={`activity-asset-bar-${asset.kind}`}
          data-fraction={fraction}
          style={{
            backgroundColor: ASSET_COLOR[asset.kind],
            height: "100%",
            width: `${Math.round(fraction * 100)}%`,
          }}
        />
      </div>
    </div>
  );
}

/**
 * Why a Blocked library is waiting, one sentence per missing model, said once
 * instead of on every row. Shared with the Now section, which has no action
 * here because its models line already links to Settings, Intelligence.
 */
export function BlockedCauses({
  activity,
  testId,
  onTurnOnModel,
}: {
  readonly activity: ActivityResponse;
  readonly testId: string;
  /** Renders one "Turn on a model" button for every cause listed. */
  readonly onTurnOnModel?: () => void;
}) {
  const { t } = useTranslation();
  const causes = blockedCauses(activity);
  if (causes.length === 0) return null;
  return (
    <div data-testid={testId} className="mem-activity-causes">
      <div className="mem-activity-causes-text">
        {causes.map((cause) => (
          <p key={cause.key}>{t(cause.key)}</p>
        ))}
      </div>
      {onTurnOnModel !== undefined && (
        <button
          type="button"
          className="mem-activity-popover-action"
          data-testid={`${testId}-action`}
          onClick={onTurnOnModel}
        >
          {t("activityStatus.turnOnModel")}
        </button>
      )}
    </div>
  );
}

export default function ActivitySummaryPopover({
  activity,
  onClose,
  onOpenActivity,
  onOpenIntelligence,
}: ActivitySummaryPopoverProps) {
  const { t, i18n } = useTranslation();

  const trust = trustSentence(activity);
  const trustText =
    trust.kind === "local"
      ? t(trust.key)
      : t(trust.key, {
          // The mapping returns keys, not prose, so the join happens here where
          // the locale's own list separator is available.
          jobs: trust.jobKeys
            .map((key) => t(key))
            .join(t("activityStatus.jobSeparator")),
          vendor: t(trust.vendorKey),
        });

  const byKind = new Map(activity.assets.map((asset) => [asset.kind, asset]));

  return (
    <div
      className="mem-popover-surface mem-activity-popover"
      data-testid="activity-summary"
      role="dialog"
      aria-label={t("activityStatus.statusLabel")}
    >
      {/* A missing model IS the headline: "Some steeping is waiting on you"
          above it only delayed the sentence that says what to do. */}
      {blockedCauses(activity).length > 0 ? (
        <BlockedCauses
          activity={activity}
          testId="activity-summary-causes"
          onTurnOnModel={
            onOpenIntelligence === undefined
              ? undefined
              : () => {
                  onOpenIntelligence();
                  onClose();
                }
          }
        />
      ) : (
        <p
          data-testid="activity-summary-headline"
          style={{
            color: "var(--mem-text)",
            fontFamily: "var(--mem-font-body)",
            fontSize: "12px",
            lineHeight: 1.45,
            margin: 0,
          }}
        >
          {t(`activityStatus.headline.${activity.state}`)}
        </p>
      )}

      <div style={{ display: "grid", gap: "10px" }}>
        {ASSET_ORDER.map((kind) => {
          const asset = byKind.get(kind);
          return asset ? (
            <AssetRow key={kind} activity={activity} asset={asset} />
          ) : null;
        })}
      </div>

      <p
        data-testid="activity-summary-trust"
        style={{
          borderTop: "1px solid var(--mem-border)",
          color: "var(--mem-text-tertiary)",
          fontFamily: "var(--mem-font-body)",
          fontSize: "10px",
          lineHeight: 1.45,
          margin: 0,
          paddingTop: "9px",
        }}
      >
        {trustText}
      </p>

      {/* Wraps as whole items: the sidebar is ~300px wide, and in some locales
          the time and the button do not fit on one line. */}
      <div style={{ alignItems: "center", display: "flex", flexWrap: "wrap", gap: "6px 8px" }}>
        <span
          data-testid="activity-summary-last"
          style={{
            color: "var(--mem-text-tertiary)",
            fontFamily: "var(--mem-font-mono)",
            fontSize: "10px",
            whiteSpace: "nowrap",
          }}
        >
          {activity.last_activity_at === null
            ? t("activityStatus.neverActive")
            : t("activityStatus.lastActivity", {
                time: relativeTime(activity.last_activity_at, t, i18n.language),
              })}
        </span>
        {onOpenActivity !== undefined && (
          <button
            type="button"
            className="mem-activity-popover-action"
            data-testid="activity-summary-open"
            onClick={() => {
              onOpenActivity();
              onClose();
            }}
          >
            {t("activityStatus.openActivity")}
          </button>
        )}
      </div>
    </div>
  );
}
