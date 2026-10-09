// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { RightSidebarResizeHandle, rightSidebarWidthForKey, rightSidebarWidthForRaw } from "./RightSidebarResizeHandle";

function renderHandle(width = 360, maxWidth = 600) {
  const onPreview = vi.fn();
  const onCommit = vi.fn();
  const onClose = vi.fn();
  const rendered = render(<RightSidebarResizeHandle enabled maxWidth={maxWidth} onClose={onClose} onCommit={onCommit} onPreview={onPreview} width={width} />);
  return { ...rendered, onPreview, onCommit, onClose };
}

function startDrag(handle: HTMLElement, x = 500, pointerId = 1) {
  fireEvent.pointerDown(handle, { pointerId, pointerType: "mouse", isPrimary: true, button: 0, clientX: x });
}

afterEach(() => {
  document.body.classList.remove("sidebar-resize-active");
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("RightSidebarResizeHandle", () => {
  it("mirrors the drag delta, clamps the committed range, and closes at the snap threshold", () => {
    expect(rightSidebarWidthForRaw(440, 500)).toBe(440);
    expect(rightSidebarWidthForRaw(700, 500)).toBe(500);
    expect(rightSidebarWidthForRaw(41, 500)).toBe(320);
    expect(rightSidebarWidthForRaw(20, 500)).toBe(0);
    expect(rightSidebarWidthForKey("ArrowLeft", 360, 400)).toBe(380);
    expect(rightSidebarWidthForKey("ArrowRight", 320, 400)).toBe("close");
    expect(rightSidebarWidthForKey("Home", 360, 400)).toBe("close");
    expect(rightSidebarWidthForKey("End", 360, 400)).toBe(400);

    const { onPreview, onCommit, onClose } = renderHandle();
    const handle = screen.getByTestId("right-sidebar-resize-handle");
    startDrag(handle, 500, 9);
    fireEvent.pointerMove(document, { pointerId: 9, clientX: 900 });
    expect(handle).toHaveAttribute("aria-valuemin", "0");
    expect(handle).toHaveAttribute("aria-valuenow", "0");
    expect(handle).toHaveAttribute("aria-valuetext", "Sidebar hidden");
    fireEvent.pointerCancel(document, { pointerId: 9 });
    expect(onCommit).not.toHaveBeenCalled();

    startDrag(handle);
    fireEvent.pointerMove(document, { pointerId: 1, clientX: 420 });
    expect(onPreview).toHaveBeenLastCalledWith(440);
    fireEvent.pointerUp(document, { pointerId: 1, clientX: 420 });
    expect(onPreview).toHaveBeenLastCalledWith(null);
    expect(onCommit).toHaveBeenCalledWith(440);
    expect(onClose).not.toHaveBeenCalled();

    startDrag(handle, 500, 2);
    fireEvent.pointerUp(document, { pointerId: 2, clientX: 900 });
    expect(onClose).toHaveBeenCalledOnce();
    expect(onCommit).toHaveBeenCalledTimes(1);
  });

  it("cancels previews on Escape, pointer cancellation, and viewport resize", () => {
    const { onPreview, onCommit } = renderHandle();
    const handle = screen.getByTestId("right-sidebar-resize-handle");
    startDrag(handle);
    fireEvent.pointerMove(document, { pointerId: 1, clientX: 420 });
    const escape = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    document.dispatchEvent(escape);
    expect(escape.defaultPrevented).toBe(true);
    expect(onPreview).toHaveBeenLastCalledWith(null);
    expect(onCommit).not.toHaveBeenCalled();
    expect(document.body).not.toHaveClass("sidebar-resize-active");

    startDrag(handle, 500, 2);
    fireEvent.pointerMove(document, { pointerId: 2, clientX: 420 });
    fireEvent.pointerCancel(document, { pointerId: 2 });
    expect(onPreview).toHaveBeenLastCalledWith(null);
    expect(onCommit).not.toHaveBeenCalled();

    startDrag(handle, 500, 3);
    fireEvent.pointerMove(document, { pointerId: 3, clientX: 420 });
    fireEvent(window, new Event("resize"));
    expect(onPreview).toHaveBeenLastCalledWith(null);
    fireEvent.pointerUp(document, { pointerId: 3, clientX: 420 });
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("commits mirrored keyboard changes, and rejects touch and narrow viewport drags", () => {
    const { onCommit, onClose } = renderHandle();
    const handle = screen.getByTestId("right-sidebar-resize-handle");
    fireEvent.keyDown(handle, { key: "ArrowLeft" });
    expect(onCommit).toHaveBeenCalledWith(380);
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(onCommit).toHaveBeenLastCalledWith(340);
    fireEvent.keyDown(handle, { key: "End" });
    expect(onCommit).toHaveBeenLastCalledWith(600);
    fireEvent.keyDown(handle, { key: "Home" });
    expect(onClose).toHaveBeenCalledOnce();

    fireEvent.pointerDown(handle, { pointerId: 4, pointerType: "touch", isPrimary: true, button: 0, clientX: 500 });
    fireEvent.pointerUp(document, { pointerId: 4, clientX: 400 });
    expect(onCommit).toHaveBeenCalledTimes(3);

    vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
    fireEvent.pointerDown(handle, { pointerId: 5, pointerType: "mouse", isPrimary: true, button: 0, clientX: 500 });
    fireEvent.pointerUp(document, { pointerId: 5, clientX: 400 });
    expect(onCommit).toHaveBeenCalledTimes(3);
  });
});
