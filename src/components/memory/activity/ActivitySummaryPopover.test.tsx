// SPDX-License-Identifier: AGPL-3.0-only
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../../i18n";
import ActivityStatus from "./ActivityStatus";
import type {
  ActivityAssetKind,
  ActivityAssetStatus,
  ActivityLane,
  ActivityResponse,
} from "../../../lib/tauri";

const getActivityMock = vi.hoisted(() => vi.fn());

vi.mock("../../../lib/tauri", () => ({
  getActivity: getActivityMock,
}));

function route(job: "everyday" | "synthesis", lane: ActivityLane, available: boolean) {
  return {
    job,
    lane,
    model: available ? "qwen3-8b" : null,
    mode: available ? "pinned" : "unconfigured",
    available,
  };
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
    assets: [asset("memories"), asset("entities"), asset("pages")],
    everyday: route("everyday", "on_device", true),
    synthesis: route("synthesis", "on_device", true),
    ...fields,
  };
}

/** Renders the Activity button and opens the popover, the way a user reaches it. */
async function openPopover(
  data: ActivityResponse,
  options: { readonly onOpenActivity?: (() => void) | null } = {},
) {
  // null means "the shell has no Activity route"; omitted means "wire a spy".
  const onOpenActivity =
    options.onOpenActivity === null ? undefined : (options.onOpenActivity ?? vi.fn());
  getActivityMock.mockResolvedValue(data);
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  function Harness() {
    const [open, setOpen] = useState(false);
    return (
      <ActivityStatus
        expanded={open}
        onToggle={() => setOpen((value) => !value)}
        onOpenActivity={onOpenActivity}
      />
    );
  }

  render(
    <QueryClientProvider client={queryClient}>
      <Harness />
    </QueryClientProvider>,
  );

  const trigger = screen.getByTestId("activity-status");
  // Before the first read the button navigates; wait until it opens a popover.
  await waitFor(() => expect(trigger).toHaveAttribute("aria-haspopup", "dialog"));
  await userEvent.click(trigger);
  return { trigger, onOpenActivity };
}

beforeEach(async () => {
  getActivityMock.mockReset();
  await i18n.changeLanguage("en");
});

describe("ActivitySummaryPopover", () => {
  it("names a missing model once, not on every row it blocks", async () => {
    // Memories and Entities both run on the everyday model. The live app
    // printed "no everyday model is loaded" under each row.
    await openPopover(
      activity({
        state: "blocked",
        everyday: route("everyday", "basic", false),
        synthesis: route("synthesis", "none", false),
        assets: [
          asset("memories", { total: 8, blocked: 8 }),
          asset("entities", { total: 8, blocked: 8 }),
          asset("pages"),
        ],
      }),
    );

    const dialog = screen.getByRole("dialog");
    expect(
      dialog.textContent?.match(/No everyday model is loaded/g) ?? [],
    ).toHaveLength(1);
    // Pages has nothing blocked, so its model is not named as a cause.
    expect(dialog).not.toHaveTextContent("No page-writing model");
    expect(screen.getByTestId("activity-asset-memories")).toHaveTextContent(
      "8 waiting for a model",
    );
    expect(screen.getByTestId("activity-asset-entities")).toHaveTextContent(
      "8 waiting for a model",
    );
  });

  it("opens a dialog with the three asset rows in a fixed order", async () => {
    await openPopover(
      activity({
        // Deliberately out of order: the popover orders the rows, not the wire.
        assets: [asset("pages"), asset("memories"), asset("entities")],
      }),
    );

    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeInTheDocument();

    const rows = ["memories", "entities", "pages"].map((kind) =>
      screen.getByTestId(`activity-asset-${kind}`),
    );
    expect(
      rows[0].compareDocumentPosition(rows[1]) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      rows[1].compareDocumentPosition(rows[2]) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("says what each asset is doing", async () => {
    await openPopover(
      activity({
        state: "organizing",
        assets: [
          asset("memories", { done: 40, total: 143 }),
          asset("entities", { done: 21, total: 21, steps: [
            { name: "confirm", state: "idle", done: 9, total: 21, failed: 0, job: null },
          ] }),
          asset("pages", { done: 12, total: 12 }),
        ],
      }),
    );

    expect(screen.getByTestId("activity-asset-memories")).toHaveTextContent(
      "40 of 143 summarized and linked",
    );
    expect(screen.getByTestId("activity-asset-entities")).toHaveTextContent(
      "21 found, 9 confirmed in the Wiki",
    );
    expect(screen.getByTestId("activity-asset-pages")).toHaveTextContent(
      "12 pages written, all current",
    );
  });

  it("names the missing model and where to choose one when blocked", async () => {
    await openPopover(
      activity({
        state: "blocked",
        assets: [
          asset("memories", { done: 143, total: 143 }),
          asset("entities", { done: 21, total: 21 }),
          asset("pages", { blocked: 12, total: 12 }),
        ],
        synthesis: route("synthesis", "none", false),
      }),
    );

    expect(screen.getByTestId("activity-asset-pages")).toHaveTextContent(
      "12 waiting for a model",
    );
    const causes = screen.getByTestId("activity-summary-causes");
    expect(causes).toHaveTextContent("No page-writing model is loaded");
    expect(causes).toHaveTextContent("Settings, Intelligence");
  });

  it("draws no progress for an asset with nothing in it", async () => {
    // 0/0 would otherwise compute as a full bar and read as finished.
    await openPopover(activity());
    expect(screen.getByTestId("activity-asset-bar-memories")).toHaveAttribute(
      "data-fraction",
      "0",
    );
  });

  it("claims nothing leaves the device only when every lane is local", async () => {
    await openPopover(activity());
    expect(screen.getByTestId("activity-summary-trust")).toHaveTextContent(
      "Nothing leaves your device",
    );
  });

  it("names the vendor and the job that leaves the device", async () => {
    await openPopover(
      activity({ synthesis: route("synthesis", "anthropic", true) }),
    );
    const trust = screen.getByTestId("activity-summary-trust");
    expect(trust).toHaveTextContent("page writing");
    expect(trust).toHaveTextContent("on Anthropic");
    expect(trust).not.toHaveTextContent("Nothing leaves your device");
  });

  it("says nothing has run yet rather than a bare time", async () => {
    await openPopover(activity());
    expect(screen.getByTestId("activity-summary-last")).toHaveTextContent(
      "Nothing has run yet",
    );
  });

  it("prints the last activity time when there is one", async () => {
    await openPopover(
      activity({ last_activity_at: Math.floor(Date.now() / 1000) - 600 }),
    );
    expect(screen.getByTestId("activity-summary-last")).toHaveTextContent(
      "10m ago",
    );
  });

  it("closes on Escape and returns focus to the Activity button", async () => {
    const { trigger } = await openPopover(activity());
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(trigger).toHaveFocus();
  });

  it("opens on Enter from the keyboard", async () => {
    getActivityMock.mockResolvedValue(activity());
    await openPopover(activity());
    // Already open from the click; close and reopen with the keyboard.
    await userEvent.keyboard("{Escape}");
    await userEvent.keyboard("{Enter}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("navigates to Activity and closes", async () => {
    const onOpenActivity = vi.fn();
    await openPopover(activity(), { onOpenActivity });

    const open = screen.getByTestId("activity-summary-open");
    expect(open).toHaveTextContent("See all activity");
    await userEvent.click(open);

    expect(onOpenActivity).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("omits See all activity when the shell has no such route", async () => {
    await openPopover(activity(), { onOpenActivity: null });
    expect(screen.queryByTestId("activity-summary-open")).toBeNull();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
