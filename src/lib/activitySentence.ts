// SPDX-License-Identifier: AGPL-3.0-only
//
// The pure state-to-copy mapping behind the status line, the popover and the
// Now section. Kept out of the components so acceptance 5 (Blocked is honest)
// and 6 (trust matches routing) are tested as functions with literal inputs
// rather than through three render trees that would each have to be mocked.
//
// Every function here returns an i18n key plus its interpolation params. None
// of them return prose: the copy lives in activityCopy.ts, and
// hardcodedCopyGuard.test.ts would fail on a literal sentence here.

import type { ParseKeys } from "i18next";
import type {
  ActivityAssetKind,
  ActivityAssetStatus,
  ActivityJob,
  ActivityLane,
  ActivityResponse,
  ActivityRoute,
} from "./tauri";

/**
 * An i18n key with its interpolation params. `key` is `ParseKeys`, not
 * `string`, so a typo or a renamed copy key fails `tsc` here rather than
 * rendering the key itself into the sidebar.
 */
export interface Phrase {
  readonly key: ParseKeys;
  readonly params?: Record<string, string | number>;
}

/**
 * The job whose lane governs an asset. Memories and Entities run on the
 * everyday route; Pages on synthesis. This mirrors `ActivityStepName::job()`
 * in wenlan-types, which is the server-side authority — Store and Confirm
 * carry no lane, so an asset is governed by whichever of its steps does.
 */
export function governingJob(kind: ActivityAssetKind): ActivityJob {
  return kind === "pages" ? "synthesis" : "everyday";
}

export function routeFor(
  activity: ActivityResponse,
  kind: ActivityAssetKind,
): ActivityRoute {
  return governingJob(kind) === "synthesis"
    ? activity.synthesis
    : activity.everyday;
}

/**
 * Which sentence an asset row gets.
 *
 * The two Blocked causes are told apart by the governing route, not by the
 * counts: an unavailable lane means the work never started (no model loaded),
 * an available one means the work ran and failed. The spec requires both to
 * name their own cause and next action, and only the route can tell them
 * apart — `blocked > 0` is true in both cases.
 */
export function assetSentence(
  activity: ActivityResponse,
  asset: ActivityAssetStatus,
): Phrase {
  const kind = asset.kind;
  const route = routeFor(activity, kind);

  if (asset.blocked > 0) {
    return route.available
      ? {
          key: `activityStatus.assetBlockedFailed.${kind}`,
          params: { count: asset.blocked, total: asset.total },
        }
      : {
          key: `activityStatus.assetBlockedNoModel.${kind}`,
          params: { count: asset.blocked },
        };
  }

  if (asset.total === 0) return { key: `activityStatus.assetEmpty.${kind}` };

  if (asset.done < asset.total) {
    return {
      key: `activityStatus.assetRunning.${kind}`,
      params: { done: asset.done, total: asset.total },
    };
  }

  if (kind === "entities") {
    // "21 found, 9 confirmed in the Wiki" — confirmed is the Confirm step's
    // own done count, not the asset's, because an entity can be detected and
    // never earn enough substance to be confirmed.
    const confirmed =
      asset.steps.find((step) => step.name === "confirm")?.done ?? 0;
    return {
      key: "activityStatus.assetDone.entities",
      params: { count: asset.total, confirmed },
    };
  }

  return {
    key: `activityStatus.assetDone.${kind}`,
    params: { count: asset.total },
  };
}

/**
 * The missing models behind a Blocked library, each named once.
 *
 * A row only says how many items are waiting; the cause and the fix belong to
 * the model, not the asset. Memories and Entities both run on the everyday
 * model, so a per-row cause repeated the same sentence. Failed work (the lane
 * is available) is not listed: its rows carry their own cause.
 */
export function blockedCauses(activity: ActivityResponse): Phrase[] {
  const jobs = new Set<ActivityJob>();
  for (const asset of activity.assets) {
    if (asset.blocked > 0 && !routeFor(activity, asset.kind).available) {
      jobs.add(governingJob(asset.kind));
    }
  }
  return (["everyday", "synthesis"] as const)
    .filter((job) => jobs.has(job))
    .map((job) => ({ key: `activityStatus.blockedCause.${job}` }));
}

/** The "Everyday work: qwen3-8b on this machine." sentence for one route. */
export function routeSentence(route: ActivityRoute): Phrase {
  return route.model === null
    ? { key: "activityStatus.routeNoModel" }
    : { key: "activityStatus.route", params: { model: route.model } };
}

/** Lane chip copy for a resolved route. */
export function laneKey(lane: ActivityLane): ParseKeys {
  return `activityStatus.lane.${lane}`;
}

const CLOUD_LANES: readonly ActivityLane[] = ["anthropic"];

export function isCloud(route: ActivityRoute): boolean {
  return CLOUD_LANES.includes(route.lane);
}

/**
 * The trust sentence, as keys rather than a Phrase.
 *
 * The cloud case interpolates a LIST of job names, and a list of keys cannot
 * ride inside `params` without being flattened to a string and losing its
 * types. So it travels as an array and the component joins it with the
 * locale's own separator.
 */
export type TrustPhrase =
  | { readonly kind: "local"; readonly key: "activityStatus.trustLocal" }
  | {
      readonly kind: "cloud";
      readonly key: "activityStatus.trustCloud";
      readonly jobKeys: readonly ParseKeys[];
      readonly vendorKey: ParseKeys;
    };

/**
 * "Nothing leaves your device" is claimed only when NO resolved lane is a
 * cloud vendor. `external` is a local server the user runs, so it stays local;
 * `basic` and `none` run no model at all. Any cloud lane names its vendor and
 * says which work leaves the device, because claiming local while sending text
 * to a vendor is the one thing this sentence must never do.
 */
export function trustSentence(activity: ActivityResponse): TrustPhrase {
  const cloud = [activity.everyday, activity.synthesis].filter(isCloud);
  if (cloud.length === 0)
    return { kind: "local", key: "activityStatus.trustLocal" };

  return {
    kind: "cloud",
    key: "activityStatus.trustCloud",
    jobKeys: cloud.map((route) => `activityStatus.job.${route.job}` as const),
    vendorKey: laneKey(cloud[0].lane),
  };
}
