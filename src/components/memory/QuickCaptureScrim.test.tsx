// SPDX-License-Identifier: AGPL-3.0-only
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import QuickCaptureScrim from "./QuickCaptureScrim";

type Handler = (event: { payload: unknown }) => void;

const listeners = vi.hoisted(() => new Map<string, Handler>());
const unlistenOpened = vi.hoisted(() => vi.fn());
const unlistenClosed = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());
const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

function fireOpened(payload: unknown): void {
  const handler = listeners.get("quick-capture-opened");
  expect(handler).toBeDefined();
  act(() => {
    handler?.({ payload });
  });
}

function fireClosed(): void {
  const handler = listeners.get("quick-capture-closed");
  expect(handler).toBeDefined();
  act(() => {
    handler?.({ payload: undefined });
  });
}

beforeEach(() => {
  listeners.clear();
  listenMock.mockReset();
  invokeMock.mockReset();
  unlistenOpened.mockReset();
  unlistenClosed.mockReset();
  listenMock.mockImplementation((event: string, handler: Handler) => {
    listeners.set(event, handler);
    if (event === "quick-capture-opened") return Promise.resolve(unlistenOpened);
    return Promise.resolve(unlistenClosed);
  });
});

describe("QuickCaptureScrim", () => {
  it("renders nothing initially", () => {
    render(<QuickCaptureScrim />);
    expect(screen.queryByTestId("quick-capture-scrim")).toBeNull();
  });

  it("shows the scrim when opened centered over main", () => {
    render(<QuickCaptureScrim />);
    fireOpened("centered-over-main");
    expect(screen.getByTestId("quick-capture-scrim")).toBeTruthy();
  });

  it("stays hidden when opened at the bottom-right shortcut position", () => {
    render(<QuickCaptureScrim />);
    fireOpened("bottom-right");
    expect(screen.queryByTestId("quick-capture-scrim")).toBeNull();
  });

  it("removes the scrim when the capture window closes", () => {
    render(<QuickCaptureScrim />);
    fireOpened("centered-over-main");
    expect(screen.getByTestId("quick-capture-scrim")).toBeTruthy();
    fireClosed();
    expect(screen.queryByTestId("quick-capture-scrim")).toBeNull();
  });

  it("dismisses the capture window on scrim click", () => {
    render(<QuickCaptureScrim />);
    fireOpened("centered-over-main");
    fireEvent.click(screen.getByTestId("quick-capture-scrim"));
    expect(invokeMock).toHaveBeenCalledWith("dismiss_quick_capture");
  });

  it("dismisses the capture window on Escape while open and marks it handled", () => {
    render(<QuickCaptureScrim />);
    fireOpened("centered-over-main");
    const event = new KeyboardEvent("keydown", { key: "Escape", cancelable: true });
    act(() => {
      window.dispatchEvent(event);
    });
    expect(invokeMock).toHaveBeenCalledWith("dismiss_quick_capture");
    expect(event.defaultPrevented).toBe(true);
  });

  it("ignores non-Escape keys while open", () => {
    render(<QuickCaptureScrim />);
    fireOpened("centered-over-main");
    const event = new KeyboardEvent("keydown", { key: "Enter", cancelable: true });
    act(() => {
      window.dispatchEvent(event);
    });
    expect(invokeMock).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("unlistens both events on unmount", async () => {
    const { unmount } = render(<QuickCaptureScrim />);
    expect(listenMock).toHaveBeenCalledTimes(2);
    await act(async () => {
      unmount();
    });
    expect(unlistenOpened).toHaveBeenCalledTimes(1);
    expect(unlistenClosed).toHaveBeenCalledTimes(1);
  });
});
