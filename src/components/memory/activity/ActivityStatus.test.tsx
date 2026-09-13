// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import ActivityStatus from "./ActivityStatus";
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
    // An icon claiming a state before asking would be a claim the app cannot
    // back, and this sits on every page. Clicking still reaches Activity.
    getActivityMock.mockReturnValue(new Promise(() => {}));
    const onOpenActivity = vi.fn();
    const { onToggle } = renderStatus(vi.fn(), false, { onOpenActivity });

    const button = screen.getByRole("button", { name: "Activity" });
    expect(button).not.toHaveAttribute("data-state");
    expect(button).not.toHaveAttribute("aria-haspopup");
    expect(screen.getByTestId("activity-status-icon")).not.toHaveAttribute("data-icon-state");

    await userEvent.click(button);
    expect(onOpenActivity).toHaveBeenCalledTimes(1);
    expect(onToggle).not.toHaveBeenCalled();
  });

  it("stays quiet when up to date: the plain clock, the state in the name", async () => {
    getActivityMock.mockResolvedValue(activity());
    renderStatus();

    const button = await loadedTrigger();
    expect(button).toHaveAttribute("data-state", "up_to_date");
    expect(button).toHaveAccessibleName("Activity, Up to date");
    expect(button).toHaveAttribute("title", "Up to date");
    expect(screen.getByTestId("activity-status-icon")).not.toHaveAttribute("data-icon-state");
    expect(button.querySelector(".mem-activity-status-badge, .mem-activity-dot")).toBeNull();
  });

  it("marks itself as the current page on the Activity view", async () => {
    getActivityMock.mockResolvedValue(activity());
    renderStatus(vi.fn(), false, { current: true });
    expect(await loadedTrigger()).toHaveAttribute("aria-current", "page");
  });

  it("shows no number in any state", async () => {
    // Memories, entities and pages count different things, so no one figure
    // on the button is true for all three. The state is the icon's form.
    for (const state of ["organizing", "blocked"] as const) {
      getActivityMock.mockResolvedValue(
        activity({
          state,
          assets: [
            asset("memories", { blocked: 3, done: 7, total: 10 }),
            asset("entities", { blocked: 8, done: 2, total: 10 }),
            asset("pages", { done: 2, total: 4 }),
          ],
        }),
      );
      const { unmount } = renderStatus();
      const button = await loadedTrigger();
      expect(button).toHaveAttribute("data-state", state);
      expect(button.textContent).toBe("Activity");
      unmount();
    }
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
    expect(line).toHaveAccessibleName("Activity, Blocked");
    expect(screen.getByTestId("activity-status-icon")).toHaveAttribute(
      "data-icon-state",
      "blocked",
    );
  });

  it("sweeps the clock hands only while steeping, and shows ! when blocked", async () => {
    getActivityMock.mockResolvedValue(activity({ state: "organizing" }));
    const { unmount } = renderStatus();
    const steeping = await screen.findByTestId("activity-status-icon");
    await waitFor(() => expect(steeping).toHaveAttribute("data-icon-state", "organizing"));
    expect(steeping.querySelector(".mem-activity-hands-sweep")).not.toBeNull();
    unmount();

    getActivityMock.mockResolvedValue(
      activity({ state: "blocked", assets: [asset("pages", { blocked: 1, total: 1 })] }),
    );
    renderStatus();
    await waitFor(() => {
      expect(screen.getByTestId("activity-status-icon")).toHaveAttribute("data-icon-state", "blocked");
    });
    const blocked = screen.getByTestId("activity-status-icon");
    // Blocked is told by shape, not color alone: the hands become "!", and it
    // does not move, because waiting work is not an alarm.
    expect(blocked.querySelector(".mem-activity-hands")).toBeNull();
    expect(blocked.querySelector(".mem-activity-hands-sweep")).toBeNull();
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
