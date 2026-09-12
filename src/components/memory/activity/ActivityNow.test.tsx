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
      "143 of 143",
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

  it("names both models and their lanes", async () => {
    renderNow(
      activity({ synthesis: route("synthesis", "anthropic", "claude-opus-5") }),
    );
    const models = await screen.findByTestId("activity-now-models");
    expect(models).toHaveTextContent("qwen3-8b on this machine");
    expect(models).toHaveTextContent("claude-opus-5 on Anthropic");
  });

  it("says no model is loaded rather than printing null", async () => {
    renderNow(activity({ synthesis: route("synthesis", "none", null) }));
    expect(await screen.findByTestId("activity-now-models")).toHaveTextContent(
      "no model loaded",
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
