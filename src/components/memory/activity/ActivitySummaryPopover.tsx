// SPDX-License-Identifier: AGPL-3.0-only
import { useTranslation } from "react-i18next";
import type { Ref } from "react";
import type { ActivityResponse } from "../../../lib/tauri";
import { relativeTime } from "../../../lib/relativeTime";
import { assetCount, assetProgress, assetSentence, knownAssets, type KnownActivityAsset, type KnownActivityAssetKind } from "../../../lib/activitySentence";

const ASSET_ORDER: readonly KnownActivityAssetKind[] = ["memories", "entities", "pages"];
const ASSET_COLOR: Record<KnownActivityAssetKind, string> = {
  memories: "var(--mem-accent-indigo)",
  entities: "var(--mem-accent-sage)",
  pages: "var(--mem-accent-warm)",
};

function AssetRow({
  activity,
  asset,
}: {
  readonly activity: ActivityResponse;
  readonly asset: KnownActivityAsset;
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
      {activity.state !== "off" && (
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
      )}
    </div>
  );
}


interface Props {
  readonly id: string;
  readonly activity?: ActivityResponse;
  readonly isError: boolean;
  readonly firstActionRef: Ref<HTMLButtonElement>;
  readonly onReadAgain: () => void;
  readonly onOpenActivity: () => void;
}

export default function ActivitySummaryPopover({ id, activity, isError, firstActionRef, onReadAgain, onOpenActivity }: Props) {
  const { t, i18n } = useTranslation();
  const byKind = new Map(activity ? knownAssets(activity).map((asset) => [asset.kind, asset]) : []);
  return (
    <div id={id} className="mem-popover-surface mem-activity-popover" data-testid="activity-summary" role="dialog" aria-label={t("activityStatus.statusLabel")}>
      <p style={{ margin: 0, fontSize: 14, fontWeight: 600, color: "var(--mem-text)" }}>{t("main.activity")}</p>
      {isError ? (
        <div>
          <p role="alert">{t("activityStatus.nowError")}</p>
          <button ref={firstActionRef} type="button" className="mem-activity-popover-action" onClick={onReadAgain}>{t("activityStatus.readAgain")}</button>
        </div>
      ) : activity === undefined ? (
        <p role="status">{t("activityStatus.nowLoading")}</p>
      ) : (
        <>
          <div style={{ display: "grid", gap: 12 }}>
            {ASSET_ORDER.map((kind) => {
              const asset = byKind.get(kind);
              return asset ? <AssetRow key={kind} activity={activity} asset={asset} /> : null;
            })}
          </div>
          <span data-testid="activity-summary-last" style={{ fontSize: 11, color: "var(--mem-text-tertiary)" }}>
            {activity.last_activity_at === null ? t("activityStatus.neverActive") : t("activityStatus.lastActivity", { time: relativeTime(activity.last_activity_at, t, i18n.language) })}
          </span>
        </>
      )}
      <button ref={isError ? undefined : firstActionRef} type="button" className="mem-activity-popover-action" data-testid="activity-summary-open" onClick={onOpenActivity}>{t("activityStatus.openActivity")}</button>
    </div>
  );
}
