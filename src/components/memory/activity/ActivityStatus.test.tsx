// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import ActivityStatus, { statusDetail } from "./ActivityStatus";
import type {
  ActivityAssetKind,
  ActivityAssetStatus,
  ActivityResponse,
} from "../../../lib/tauri";

const getActivityMock = vi.hoisted(() => vi.fn());

vi.mock("../../../lib/tauri", () => ({
  getActivity: getActivityMock,
}));

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

function activity(fields: Partial<ActivityResponse> = {}): ActivityResponse {
  return {
    state: "up_to_date",
    last_activity_at: null,
    assets: [],
    everyday: {
      job: "everyday",
      lane: "on_device",
      model: "a-model",
      mode: "pinned",
      available: true,
    },
    synthesis: {
      job: "synthesis",
      lane: "on_device",
      model: "a-model",
      mode: "pinned",
      available: true,
    },
    ...fields,
  };
}

function renderStatus(
  onToggle = vi.fn(),
  expanded = false,
  props: { readonly current?: boolean; readonly onOpenActivity?: () => void } = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const view = render(
    <QueryClientProvider client={queryClient}>
      <ActivityStatus expanded={expanded} onToggle={onToggle} {...props} />
    </QueryClientProvider>,
  );
  return { ...view, onToggle };
}

/** The trigger once the first read has landed and it carries a state. */
async function loadedTrigger() {
  const trigger = screen.getByTestId("activity-status");
  await waitFor(() => expect(trigger).toHaveAttribute("data-state"));
  return trigger;
}

beforeEach(() => {
  getActivityMock.mockReset();
});

describe("ActivityStatus", () => {
  it("is the plain Activity button until the first read lands", async () => {
    // A dot claiming a state before asking would be a claim the app cannot
    // back, and this sits on every page. Clicking still reaches Activity.
    getActivityMock.mockReturnValue(new Promise(() => {}));
    const onOpenActivity = vi.fn();
    const { onToggle } = renderStatus(vi.fn(), false, { onOpenActivity });

    const button = screen.getByRole("button", { name: "Activity" });
    expect(button).not.toHaveAttribute("data-state");
    expect(button).not.toHaveAttribute("aria-haspopup");
    expect(screen.queryByTestId("activity-status-dot")).toBeNull();

    await userEvent.click(button);
    expect(onOpenActivity).toHaveBeenCalledTimes(1);
    expect(onToggle).not.toHaveBeenCalled();
  });

  it("stays quiet when up to date: the state is in the name, not a dot", async () => {
    getActivityMock.mockResolvedValue(activity());
    renderStatus();

    const button = await loadedTrigger();
    expect(button).toHaveAttribute("data-state", "up_to_date");
    expect(button).toHaveAccessibleName("Activity, Up to date");
    expect(button).toHaveAttribute("title", "Up to date");
    expect(screen.queryByTestId("activity-status-dot")).toBeNull();
    expect(screen.queryByTestId("activity-status-detail")).toBeNull();
  });

  it("marks itself as the current page on the Activity view", async () => {
    getActivityMock.mockResolvedValue(activity());
    renderStatus(vi.fn(), false, { current: true });
    expect(await loadedTrigger()).toHaveAttribute("aria-current", "page");
  });

  it("shows the busiest asset's progress while steeping", async () => {
    getActivityMock.mockResolvedValue(
      activity({
        state: "organizing",
        assets: [
          // Most work left is 8, on pages, even though memories is listed first.
          asset("memories", { done: 9, total: 10 }),
          asset("pages", { done: 2, total: 10 }),
        ],
      }),
    );
    renderStatus();

    expect(await screen.findByTestId("activity-status-detail")).toHaveTextContent(
      "2/10",
    );
  });

  it("shows the total stuck count when blocked", async () => {
    getActivityMock.mockResolvedValue(
      activity({
        state: "blocked",
        assets: [
          asset("memories", { blocked: 3, total: 10, done: 7 }),
          asset("pages", { blocked: 2, total: 4, done: 2 }),
        ],
      }),
    );
    renderStatus();

    const line = await loadedTrigger();
    expect(line).toHaveAttribute("data-state", "blocked");
    expect(screen.getByTestId("activity-status-detail")).toHaveTextContent("5");
  });

  it("reports Blocked even when other assets are still steeping", async () => {
    // The daemon decides the overall state; the button must not recompute it
    // from the counts and quietly downgrade a blocked library to Steeping.
    getActivityMock.mockResolvedValue(
      activity({
        state: "blocked",
        assets: [
          asset("memories", { done: 1, total: 9 }),
          asset("entities", { blocked: 4, total: 4, done: 0 }),
        ],
      }),
    );
    renderStatus();

    const line = await loadedTrigger();
    expect(line).toHaveAttribute("data-state", "blocked");
    expect(screen.getByTestId("activity-status-detail")).toHaveTextContent("4");
  });

  it("pulses the dot only while steeping", async () => {
    getActivityMock.mockResolvedValue(activity({ state: "organizing" }));
    const { unmount } = renderStatus();
    expect(await screen.findByTestId("activity-status-dot")).toHaveClass(
      "mem-activity-dot-pulse",
    );
    unmount();

    getActivityMock.mockResolvedValue(
      activity({ state: "blocked", assets: [asset("pages", { blocked: 1, total: 1 })] }),
    );
    renderStatus();
    await waitFor(() => {
      expect(screen.getByTestId("activity-status-dot")).not.toHaveClass(
        "mem-activity-dot-pulse",
      );
    });
  });

  it("opens the popover on click and reports its expanded state", async () => {
    getActivityMock.mockResolvedValue(activity());
    const { onToggle } = renderStatus();

    const line = await loadedTrigger();
    expect(line).toHaveAttribute("aria-haspopup", "dialog");
    expect(line).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(line);
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("marks itself expanded when the popover is open", async () => {
    getActivityMock.mockResolvedValue(activity());
    renderStatus(vi.fn(), true);
    expect(await loadedTrigger()).toHaveAttribute("aria-expanded", "true");
  });

  it("closes when the user clicks outside it", async () => {
    getActivityMock.mockResolvedValue(activity());
    const { onToggle } = renderStatus(vi.fn(), true);
    await loadedTrigger();

    await userEvent.click(screen.getByRole("dialog"));
    expect(onToggle).not.toHaveBeenCalled();

    await userEvent.click(document.body);
    expect(onToggle).toHaveBeenCalledTimes(1);
  });
});

describe("statusDetail", () => {
  it("gives no number when everything is organized", () => {
    expect(statusDetail(activity())).toBeNull();
  });

  it("gives no number when steeping with nothing outstanding", () => {
    // Defensive: the daemon can report organizing on the tick where the last
    // item finished. An empty busiest asset must not render "undefined".
    expect(
      statusDetail(
        activity({
          state: "organizing",
          assets: [asset("memories", { done: 4, total: 4 })],
        }),
      ),
    ).toBeNull();
  });

  it("gives no number when blocked with no stuck items", () => {
    expect(
      statusDetail(activity({ state: "blocked", assets: [asset("pages")] })),
    ).toBeNull();
  });
});
