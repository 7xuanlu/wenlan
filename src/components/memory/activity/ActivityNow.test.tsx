// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../../i18n";
import ActivityNow, { effectiveLayout } from "./ActivityNow";
import {
  __resetActivityNowLayoutForTests,
  setActivityNowLayout,
} from "../../../lib/activityNowLayout";
import type {
  ActivityAssetKind,
  ActivityAssetStatus,
  ActivityLane,
  ActivityResponse,
  ActivityStep,
  ActivityStepName,
} from "../../../lib/tauri";

const getActivityMock = vi.hoisted(() => vi.fn());

vi.mock("../../../lib/tauri", () => ({
  getActivity: getActivityMock,
}));

/** jsdom has no matchMedia; give it one whose width the test controls. */
function stubWidth(wide: boolean) {
  vi.stubGlobal(
    "matchMedia",
    (query: string) =>
      ({
        matches: query.includes("min-width") ? wide : false,
        media: query,
        addEventListener: () => {},
        removeEventListener: () => {},
      }) as unknown as MediaQueryList,
  );
}

function route(job: "everyday" | "synthesis", lane: ActivityLane, model: string | null) {
  return {
    job,
    lane,
    model,
    mode: model === null ? "unconfigured" : "pinned",
    available: model !== null,
  };
}

function step(
  name: ActivityStepName,
  fields: Partial<ActivityStep> = {},
): ActivityStep {
  return { name, state: "idle", done: 0, total: 0, failed: 0, job: null, ...fields };
}

function asset(
  kind: ActivityAssetKind,
  fields: Partial<ActivityAssetStatus> = {},
): ActivityAssetStatus {
  return { kind, state: "idle", done: 0, total: 0, blocked: 0, steps: [], ...fields };
}

function activity(fields: Partial<ActivityResponse> = {}): ActivityResponse {
  return {
    state: "up_to_date",
    last_activity_at: null,
    assets: [
      asset("memories", {
        done: 143,
        total: 143,
        steps: [
          step("store", { done: 143, total: 143 }),
          step("summarize", { done: 143, total: 143, job: "everyday" }),
          step("link", { done: 143, total: 143, job: "everyday" }),
        ],
      }),
      asset("entities", { done: 21, total: 21 }),
      asset("pages", { done: 12, total: 12 }),
    ],
    everyday: route("everyday", "on_device", "qwen3-8b"),
    synthesis: route("synthesis", "on_device", "qwen3-8b"),
    refinement: { ready_for_review: 0, not_ready: 0, groups: [] },
    ...fields,
  };
}

function renderNow(
  data: ActivityResponse = activity(),
  onOpenIntelligence?: () => void,
) {
  getActivityMock.mockResolvedValue(data);
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <ActivityNow onOpenIntelligence={onOpenIntelligence} />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  getActivityMock.mockReset();
  localStorage.clear();
  __resetActivityNowLayoutForTests();
  stubWidth(true);
  await i18n.changeLanguage("en");
});

describe("effectiveLayout", () => {
  it("falls back from rail to card on a narrow window", () => {
    expect(effectiveLayout("rail", false)).toBe("card");
    expect(effectiveLayout("rail", true)).toBe("rail");
  });

  it("leaves the other two layouts alone at any width", () => {
    expect(effectiveLayout("card", false)).toBe("card");
    expect(effectiveLayout("timeline", false)).toBe("timeline");
  });
});

describe("ActivityNow", () => {
  it("defaults to the rail", async () => {
    renderNow();
    expect(await screen.findByTestId("activity-now")).toHaveAttribute(
      "data-layout",
      "rail",
    );
  });

  it("renders the card when Settings says card", async () => {
    setActivityNowLayout("card");
    renderNow();
    expect(await screen.findByTestId("activity-now")).toHaveAttribute(
      "data-layout",
      "card",
    );
  });

  it("renders the timeline group without the card surface", async () => {
    setActivityNowLayout("timeline");
    renderNow();
    const section = await screen.findByTestId("activity-now");
    expect(section).toHaveAttribute("data-layout", "timeline");
    expect(section).not.toHaveClass("mem-activity-now-surface");
  });

  it("changes layout without a reload when Settings changes", async () => {
    renderNow();
    expect(await screen.findByTestId("activity-now")).toHaveAttribute(
      "data-layout",
      "rail",
    );

    act(() => {
      setActivityNowLayout("timeline");
    });

    expect(screen.getByTestId("activity-now")).toHaveAttribute(
      "data-layout",
      "timeline",
    );
  });

  it("renders the rail as a card on a narrow window", async () => {
    stubWidth(false);
    renderNow();
    expect(await screen.findByTestId("activity-now")).toHaveAttribute(
      "data-layout",
      "card",
    );
  });

  it("expands and collapses an asset's steps", async () => {
    renderNow();
    const toggle = await screen.findByTestId("activity-steps-toggle-memories");

    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("activity-steps-memories")).toBeNull();

    await userEvent.click(toggle);

    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByTestId("activity-steps-memories")).toBeInTheDocument();
    expect(screen.getByTestId("activity-step-summarize")).toHaveTextContent(
      "summary and tags so recall is accurate",
    );
    expect(screen.getByTestId("activity-step-count-summarize")).toHaveTextContent(
      "143 of 143 memories",
    );

    await userEvent.click(toggle);
    expect(screen.queryByTestId("activity-steps-memories")).toBeNull();
  });

  it("chips the lane only on steps that run a model", async () => {
    renderNow();
    await userEvent.click(
      await screen.findByTestId("activity-steps-toggle-memories"),
    );

    // Store runs no model, so it carries no lane to name.
    expect(screen.queryByTestId("activity-step-lane-store")).toBeNull();
    expect(screen.getByTestId("activity-step-lane-summarize")).toHaveTextContent(
      "on this machine",
    );
  });

  it("offers no steps toggle for an asset the daemon sent no steps for", async () => {
    renderNow();
    await screen.findByTestId("activity-now");
    expect(screen.queryByTestId("activity-steps-toggle-entities")).toBeNull();
  });

  it("drops only the headline for a state from a newer daemon", async () => {
    renderNow(activity({ state: "unknown" }));
    expect(await screen.findByTestId("activity-now-models")).toBeInTheDocument();
    expect(screen.getByTestId("activity-now-trust")).toBeInTheDocument();
    expect(screen.getByTestId("activity-steps-toggle-memories")).toBeInTheDocument();
    expect(screen.queryByTestId("activity-now-headline")).toBeNull();
  });

  it("names both models and their lanes", async () => {
    renderNow(
      activity({ synthesis: route("synthesis", "anthropic", "claude-opus-5") }),
    );
    const models = await screen.findByTestId("activity-now-models");
    expect(models).toHaveTextContent("qwen3-8b on this machine");
    expect(models).toHaveTextContent("claude-opus-5 on Anthropic");
  });

  it("says no model is in use rather than printing null", async () => {
    renderNow(activity({ synthesis: route("synthesis", "none", null) }));
    expect(await screen.findByTestId("activity-now-models")).toHaveTextContent(
      "no model in use",
    );
  });

  it("links to Settings, Intelligence", async () => {
    const onOpenIntelligence = vi.fn();
    renderNow(activity(), onOpenIntelligence);
    await userEvent.click(
      await screen.findByTestId("activity-now-intelligence"),
    );
    expect(onOpenIntelligence).toHaveBeenCalledTimes(1);
  });

  it("names the cloud vendor in the trust line when one lane is cloud", async () => {
    renderNow(
      activity({ synthesis: route("synthesis", "anthropic", "claude-opus-5") }),
    );
    const trust = await screen.findByTestId("activity-now-trust");
    expect(trust).toHaveTextContent("on Anthropic");
    expect(trust).not.toHaveTextContent("Nothing leaves your device");
  });

  // A lane this app cannot name may be a cloud vendor, so claiming the work
  // stays on the device would be false.
  it("drops the trust line, chip and model for a lane from a newer daemon", async () => {
    renderNow(activity({ everyday: route("everyday", "unknown", "new-model") }));
    const models = await screen.findByTestId("activity-now-models");
    expect(models).toHaveTextContent("qwen3-8b on this machine");
    expect(models).not.toHaveTextContent("new-model");
    expect(screen.queryByTestId("activity-now-trust")).toBeNull();

    await userEvent.click(screen.getByTestId("activity-steps-toggle-memories"));
    expect(screen.getByTestId("activity-step-summarize")).toBeInTheDocument();
    expect(screen.queryByTestId("activity-step-lane-summarize")).toBeNull();
  });

  it("leaves out a step from a newer daemon", async () => {
    const base = activity();
    renderNow(
      activity({
        assets: [
          {
            ...base.assets[0],
            steps: [...base.assets[0].steps, step("unknown", { done: 1, total: 1 })],
          },
          asset("unknown", { done: 3, total: 3 }),
        ],
      }),
    );
    await userEvent.click(await screen.findByTestId("activity-steps-toggle-memories"));
    expect(screen.getByTestId("activity-step-summarize")).toBeInTheDocument();
    expect(screen.queryByTestId("activity-step-unknown")).toBeNull();
  });

  it("hides Suggestions when no suggestion is open", async () => {
    renderNow();
    await screen.findByTestId("activity-now");
    expect(screen.queryByTestId("activity-now-suggestions")).toBeNull();
  });

  it("shows only the Suggestions lines whose count is not zero", async () => {
    renderNow(
      activity({ refinement: { ready_for_review: 0, not_ready: 3, groups: [] } }),
    );
    const block = await screen.findByTestId("activity-now-suggestions");
    expect(block).toHaveTextContent("Suggestions");
    expect(screen.queryByTestId("activity-now-suggestions-ready")).toBeNull();
    expect(screen.getByTestId("activity-now-suggestions-not-ready")).toHaveTextContent(
      "3 not yet ready for review",
    );
  });

  it("counts Suggestions in the singular and plural", async () => {
    renderNow(
      activity({ refinement: { ready_for_review: 1, not_ready: 1, groups: [] } }),
    );
    expect(
      await screen.findByTestId("activity-now-suggestions-ready"),
    ).toHaveTextContent("1 waiting for your review");
    expect(screen.getByTestId("activity-now-suggestions-not-ready")).toHaveTextContent(
      "1 not yet ready for review",
    );
  });

  it("words Suggestions per locale for one and for three", async () => {
    await i18n.changeLanguage("zh-Hant");
    const { unmount } = renderNow(
      activity({ refinement: { ready_for_review: 1, not_ready: 3, groups: [] } }),
    );
    expect(await screen.findByTestId("activity-now-suggestions")).toHaveTextContent("建議");
    expect(screen.getByTestId("activity-now-suggestions-ready")).toHaveTextContent(
      "1 則等待你審閱",
    );
    expect(screen.getByTestId("activity-now-suggestions-not-ready")).toHaveTextContent(
      "3 則尚未進入審閱",
    );
    unmount();

    await i18n.changeLanguage("en");
    renderNow(
      activity({ refinement: { ready_for_review: 3, not_ready: 0, groups: [] } }),
    );
    expect(
      await screen.findByTestId("activity-now-suggestions-ready"),
    ).toHaveTextContent("3 waiting for your review");
    expect(screen.queryByTestId("activity-now-suggestions-not-ready")).toBeNull();
  });

  it("renders nothing until the first read lands", () => {
    getActivityMock.mockReturnValue(new Promise(() => {}));
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const { container } = render(
      <QueryClientProvider client={queryClient}>
        <ActivityNow />
      </QueryClientProvider>,
    );
    expect(container).toBeEmptyDOMElement();
  });
});
