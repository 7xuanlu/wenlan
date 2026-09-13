// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import { i18n } from "../i18n";
import {
  assetSentence,
  governingJob,
  isCloud,
  laneKey,
  routeFor,
  trustSentence,
} from "./activitySentence";
import type {
  ActivityAssetKind,
  ActivityAssetStatus,
  ActivityLane,
  ActivityResponse,
  ActivityRoute,
  ActivityStep,
} from "./tauri";

function route(
  job: "everyday" | "synthesis",
  lane: ActivityLane,
  available: boolean,
): ActivityRoute {
  return {
    job,
    lane,
    model: available ? "a-model" : null,
    mode: available ? "pinned" : "unconfigured",
    available,
  };
}

function asset(
  kind: ActivityAssetKind,
  fields: Partial<ActivityAssetStatus> = {},
): ActivityAssetStatus {
  return {
    kind,
    state: "idle",
    done: 0,
    total: 0,
    blocked: 0,
    steps: [],
    ...fields,
  };
}

function step(name: ActivityStep["name"], done: number): ActivityStep {
  return { name, state: "idle", done, total: done, failed: 0, job: null };
}

function activity(fields: Partial<ActivityResponse> = {}): ActivityResponse {
  return {
    state: "up_to_date",
    last_activity_at: null,
    assets: [],
    everyday: route("everyday", "on_device", true),
    synthesis: route("synthesis", "on_device", true),
    ...fields,
  };
}

describe("governingJob / routeFor", () => {
  it("routes pages through synthesis and everything else through everyday", () => {
    expect(governingJob("pages")).toBe("synthesis");
    expect(governingJob("memories")).toBe("everyday");
    expect(governingJob("entities")).toBe("everyday");

    const a = activity({
      everyday: route("everyday", "on_device", true),
      synthesis: route("synthesis", "anthropic", true),
    });
    expect(routeFor(a, "pages").lane).toBe("anthropic");
    expect(routeFor(a, "memories").lane).toBe("on_device");
  });
});

describe("assetSentence", () => {
  it("tells the two Blocked causes apart by the route, not the counts", () => {
    // Identical asset, opposite routes: the only difference is whether a model
    // is loaded, which is exactly what the user needs told apart.
    const stuck = asset("memories", { blocked: 3, total: 10, done: 7 });

    expect(assetSentence(activity(), stuck)).toEqual({
      key: "activityStatus.assetBlockedFailed.memories",
      params: { count: 3, total: 10 },
    });

    const noModel = activity({
      everyday: route("everyday", "none", false),
    });
    expect(assetSentence(noModel, stuck)).toEqual({
      key: "activityStatus.assetBlockedNoModel.memories",
      params: { count: 3 },
    });
  });

  it("reads the pages route, not the everyday route, for a blocked page", () => {
    const a = activity({
      everyday: route("everyday", "on_device", true),
      synthesis: route("synthesis", "none", false),
    });
    expect(assetSentence(a, asset("pages", { blocked: 2, total: 2 })).key).toBe(
      "activityStatus.assetBlockedNoModel.pages",
    );
  });

  it("prefers Blocked over every other sentence", () => {
    // done === total and blocked > 0 at once: the finished-looking counts must
    // not win, or a stuck asset would read as done.
    const both = asset("entities", { blocked: 1, total: 4, done: 4 });
    expect(assetSentence(activity(), both).key).toBe(
      "activityStatus.assetBlockedFailed.entities",
    );
  });

  it("says empty when nothing has arrived", () => {
    expect(assetSentence(activity(), asset("pages"))).toEqual({
      key: "activityStatus.assetEmpty.pages",
    });
  });

  it("says running while work is outstanding", () => {
    expect(
      assetSentence(activity(), asset("memories", { done: 4, total: 9 })),
    ).toEqual({
      key: "activityStatus.assetRunning.memories",
      params: { done: 4, total: 9 },
    });
  });

  it("takes the entity confirmed count from the confirm step", () => {
    const entities = asset("entities", {
      done: 21,
      total: 21,
      steps: [step("detect", 21), step("confirm", 9)],
    });
    expect(assetSentence(activity(), entities)).toEqual({
      key: "activityStatus.assetDone.entities",
      params: { count: 21, confirmed: 9 },
    });
  });

  it("reports zero confirmed when the confirm step is absent", () => {
    const entities = asset("entities", { done: 3, total: 3 });
    expect(assetSentence(activity(), entities).params).toEqual({
      count: 3,
      confirmed: 0,
    });
  });
});

describe("trustSentence", () => {
  it("claims local only when no resolved lane is a cloud vendor", () => {
    const lanes: ActivityLane[] = ["on_device", "external", "basic", "none"];
    for (const lane of lanes) {
      const a = activity({
        everyday: route("everyday", lane, lane !== "none"),
        synthesis: route("synthesis", lane, lane !== "none"),
      });
      expect(trustSentence(a)).toEqual({ key: "activityStatus.trustLocal" });
    }
  });

  it("names the vendor and the job when one lane is cloud", () => {
    const a = activity({
      everyday: route("everyday", "on_device", true),
      synthesis: route("synthesis", "anthropic", true),
    });
    expect(trustSentence(a)).toEqual({
      key: "activityStatus.trustCloud",
      params: {
        jobsKey: "activityStatus.job.synthesis",
        vendorKey: "activityStatus.lane.anthropic",
      },
    });
  });

  it("lists both jobs when both run in the cloud", () => {
    const a = activity({
      everyday: route("everyday", "anthropic", true),
      synthesis: route("synthesis", "anthropic", true),
    });
    expect(trustSentence(a).params?.jobsKey).toBe(
      "activityStatus.job.everyday|activityStatus.job.synthesis",
    );
  });

  it("treats a local server as local, not as a vendor", () => {
    expect(isCloud(route("everyday", "external", true))).toBe(false);
    expect(isCloud(route("everyday", "anthropic", true))).toBe(true);
  });
});

describe("key coverage", () => {
  // Every key these functions can return must exist, or the surface renders the
  // key string itself. i18n parity is covered elsewhere; this checks that the
  // mapping and the copy agree on names.
  const kinds: ActivityAssetKind[] = ["memories", "entities", "pages"];
  const lanes: ActivityLane[] = [
    "on_device",
    "external",
    "anthropic",
    "basic",
    "none",
  ];

  it("resolves every asset sentence key", () => {
    const shapes: ActivityAssetStatus[] = kinds.flatMap((kind) => [
      asset(kind),
      asset(kind, { done: 1, total: 4 }),
      asset(kind, { done: 4, total: 4 }),
      asset(kind, { done: 1, total: 4, blocked: 3 }),
    ]);
    for (const available of [true, false]) {
      const a = activity({
        everyday: route("everyday", available ? "on_device" : "none", available),
        synthesis: route(
          "synthesis",
          available ? "on_device" : "none",
          available,
        ),
      });
      for (const shape of shapes) {
        const phrase = assetSentence(a, shape);
        expect(i18n.exists(phrase.key, phrase.params)).toBe(true);
      }
    }
  });

  it("resolves every lane key", () => {
    for (const lane of lanes) expect(i18n.exists(laneKey(lane))).toBe(true);
  });
});
