// SPDX-License-Identifier: AGPL-3.0-only
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const emitMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());
const listeners = vi.hoisted(() => new Map<string, (event: { payload: unknown }) => void>());

vi.mock("@tauri-apps/api/event", () => ({
  emit: emitMock,
  listen: listenMock,
}));

import UpdaterDialog from "./UpdaterDialog";

describe("UpdaterDialog", () => {
  beforeEach(() => {
    emitMock.mockReset().mockResolvedValue(undefined);
    listeners.clear();
    listenMock.mockReset().mockImplementation((event, callback) => {
      listeners.set(event, callback);
      return Promise.resolve(() => listeners.delete(event));
    });
  });

  it("announces that the updater UI is ready after mounting", async () => {
    render(<UpdaterDialog />);

    await waitFor(() => expect(emitMock).toHaveBeenCalledWith("updater://ui-ready"));
  });

  it("offers a manual check again after an install failure", async () => {
    render(<UpdaterDialog />);

    await waitFor(() => expect(emitMock).toHaveBeenCalledWith("updater://ui-ready"));
    emitMock.mockClear();

    act(() => {
      listeners.get("updater://available")?.({ payload: { version: "0.18.5" } });
      listeners.get("updater://progress")?.({ payload: { error: "network unavailable" } });
    });

    expect(screen.getByText("network unavailable")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Install" }));

    await waitFor(() => expect(emitMock).toHaveBeenCalledWith("updater://check-now"));

    act(() => {
      listeners.get("updater://status")?.({ payload: { state: "current" } });
    });
    expect(screen.queryByText("Wenlan v0.18.5 is ready.")).not.toBeInTheDocument();
  });
});
