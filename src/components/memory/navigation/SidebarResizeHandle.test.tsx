// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SidebarResizeHandle, sidebarPreferencesForKey, sidebarPreferencesForRawWidth } from "./SidebarResizeHandle";
import type { SidebarPreferences } from "./navigationPreferences";

const labels300: SidebarPreferences = { visible: true, mode: "labels", width: 300 };

function renderHandle(preferences = labels300, enabled = true) {
  const onPreview = vi.fn();
  const onCommit = vi.fn();
  const onHidden = vi.fn();
  const result = render(
    <SidebarResizeHandle preferences={preferences} enabled={enabled} onPreview={onPreview} onCommit={onCommit} onHidden={onHidden} />,
  );
  return { ...result, onPreview, onCommit, onHidden };
}

function dragStart(handle: HTMLElement, pointerId = 1, clientX = 500) {
  fireEvent.pointerDown(handle, { pointerId, pointerType: "mouse", isPrimary: true, button: 0, clientX });
}

afterEach(() => {
  document.body.classList.remove("sidebar-resize-active");
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("sidebar resize snapping", () => {
  it("maps raw widths to hidden, icon, and clamped label states", () => {
    expect(sidebarPreferencesForRawWidth(40, labels300)).toEqual({ ...labels300, visible: false });
    expect(sidebarPreferencesForRawWidth(41, labels300)).toEqual({ ...labels300, visible: true, mode: "icons" });
    expect(sidebarPreferencesForRawWidth(160, labels300)).toEqual({ ...labels300, visible: true, mode: "icons" });
    expect(sidebarPreferencesForRawWidth(161, labels300)).toEqual({ ...labels300, width: 200 });
    expect(sidebarPreferencesForRawWidth(500, labels300)).toEqual({ ...labels300, width: 360 });
  });

  it("keeps gesture-start width when a labels sidebar passes through icons and ends hidden", () => {
    const { onPreview, onCommit, onHidden } = renderHandle();
    const handle = screen.getByRole("separator");

    dragStart(handle, 1, 500);
    expect(onPreview).not.toHaveBeenCalled();
    expect(onCommit).not.toHaveBeenCalled();

    fireEvent.pointerMove(document, { pointerId: 1, clientX: 400 }); // 200px labels
    expect(onPreview).toHaveBeenLastCalledWith({ ...labels300, width: 200 });
    fireEvent.pointerMove(document, { pointerId: 1, clientX: 250 }); // icon snap
    expect(onPreview).toHaveBeenLastCalledWith({ ...labels300, mode: "icons" });
    fireEvent.pointerMove(document, { pointerId: 1, clientX: 230 }); // hidden snap
    expect(onPreview).toHaveBeenLastCalledWith({ ...labels300, visible: false });
    expect(onCommit).not.toHaveBeenCalled();

    fireEvent.pointerUp(document, { pointerId: 1, clientX: 230 });
    expect(onPreview).toHaveBeenLastCalledWith(null);
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith({ ...labels300, visible: false });
    expect(onHidden).toHaveBeenCalledTimes(1);
    expect(document.body).not.toHaveClass("sidebar-resize-active");
  });

  it("commits the label width only when the gesture ends expanded", () => {
    const { onPreview, onCommit } = renderHandle();
    dragStart(screen.getByRole("separator"), 2, 300);
    fireEvent.pointerMove(document, { pointerId: 2, clientX: 350 });
    fireEvent.pointerUp(document, { pointerId: 2, clientX: 350 });
    expect(onCommit).toHaveBeenCalledWith({ ...labels300, width: 350 });
    expect(onPreview).toHaveBeenLastCalledWith(null);
  });

  it("rolls back on Escape and pointer cancellation without committing", () => {
    const { onPreview, onCommit } = renderHandle();
    const handle = screen.getByRole("separator");
    dragStart(handle, 3, 300);
    fireEvent.pointerMove(document, { pointerId: 3, clientX: 350 });

    const escape = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
    const stopImmediate = vi.spyOn(escape, "stopImmediatePropagation");
    document.dispatchEvent(escape);
    expect(escape.defaultPrevented).toBe(true);
    expect(stopImmediate).toHaveBeenCalled();
    expect(onPreview).toHaveBeenLastCalledWith(null);
    expect(onCommit).not.toHaveBeenCalled();

    dragStart(handle, 4, 300);
    fireEvent.pointerMove(document, { pointerId: 4, clientX: 350 });
    fireEvent.pointerCancel(document, { pointerId: 4 });
    expect(onPreview).toHaveBeenLastCalledWith(null);
    expect(onCommit).not.toHaveBeenCalled();
  });

  it("rejects pointerup after a narrow viewport change before the enabled prop updates", () => {
    const { onPreview, onCommit } = renderHandle();
    dragStart(screen.getByRole("separator"), 8, 300);
    fireEvent.pointerMove(document, { pointerId: 8, clientX: 340 });
    vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
    fireEvent.pointerUp(document, { pointerId: 8, clientX: 340 });
    expect(onCommit).not.toHaveBeenCalled();
    expect(onPreview).toHaveBeenLastCalledWith(null);
    expect(document.body).not.toHaveClass("sidebar-resize-active");
  });

  it("preserves icons mode when hidden by drag and only offers the seam while visible", () => {
    const { onCommit, rerender } = renderHandle({ visible: true, mode: "icons", width: 315 });
    const handle = screen.getByRole("separator");
    dragStart(handle, 5, 100);
    fireEvent.pointerUp(document, { pointerId: 5, clientX: 50 });
    expect(onCommit).toHaveBeenCalledWith({ visible: false, mode: "icons", width: 315 });

    rerender(<SidebarResizeHandle preferences={{ visible: false, mode: "icons", width: 315 }} enabled onPreview={vi.fn()} onCommit={vi.fn()} onHidden={vi.fn()} />);
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
  });

  it("cancels if it becomes disabled and ignores unsupported pointer starts", () => {
    const { onPreview, onCommit, rerender } = renderHandle();
    const handle = screen.getByRole("separator");
    fireEvent.pointerDown(handle, { pointerId: 6, pointerType: "touch", isPrimary: true, button: 0, clientX: 300 });
    fireEvent.pointerUp(document, { pointerId: 6, clientX: 350 });
    expect(onCommit).not.toHaveBeenCalled();

    dragStart(handle, 7, 300);
    fireEvent.pointerMove(document, { pointerId: 7, clientX: 350 });
    rerender(<SidebarResizeHandle preferences={labels300} enabled={false} onPreview={onPreview} onCommit={onCommit} onHidden={vi.fn()} />);
    expect(onPreview).toHaveBeenLastCalledWith(null);
    expect(onCommit).not.toHaveBeenCalled();
    expect(screen.queryByRole("separator")).not.toBeInTheDocument();
  });

  it("keeps an active pointer gesture through parent callback changes", () => {
    const first = renderHandle();
    const handle = screen.getByRole("separator");
    dragStart(handle, 8, 300);
    const nextPreview = vi.fn();
    const nextCommit = vi.fn();
    const nextHidden = vi.fn();
    first.rerender(<SidebarResizeHandle preferences={labels300} enabled onPreview={nextPreview} onCommit={nextCommit} onHidden={nextHidden} />);

    fireEvent.pointerMove(document, { pointerId: 8, clientX: 350 });
    fireEvent.pointerUp(document, { pointerId: 8, clientX: 350 });
    expect(nextPreview).toHaveBeenCalledWith({ ...labels300, width: 350 });
    expect(nextPreview).toHaveBeenLastCalledWith(null);
    expect(nextCommit).toHaveBeenCalledWith({ ...labels300, width: 350 });
    expect(first.onCommit).not.toHaveBeenCalled();
  });

  it("rolls back if the window loses focus during a drag", () => {
    const { onPreview, onCommit } = renderHandle();
    dragStart(screen.getByRole("separator"), 9, 300);
    fireEvent.pointerMove(document, { pointerId: 9, clientX: 350 });
    fireEvent(window, new Event("blur"));
    fireEvent.pointerUp(document, { pointerId: 9, clientX: 350 });
    expect(onPreview).toHaveBeenLastCalledWith(null);
    expect(onCommit).not.toHaveBeenCalled();
    expect(document.body).not.toHaveClass("sidebar-resize-active");
  });
});

describe("sidebar resize keyboard", () => {
  it.each([
    ["ArrowRight", { visible: true, mode: "icons" }, { visible: true, mode: "labels", width: 200 }],
    ["ArrowLeft", { visible: true, mode: "labels", width: 200 }, { visible: true, mode: "icons", width: 200 }],
    ["ArrowLeft", { visible: true, mode: "icons", width: 300 }, { visible: false, mode: "icons", width: 300 }],
    ["Home", labels300, { ...labels300, visible: false }],
    ["End", labels300, { ...labels300, width: 360 }],
  ] as const)("maps %s from sidebar state to the next state", (key, current, expected) => {
    expect(sidebarPreferencesForKey(key, current)).toEqual(expected);
  });

  it("exposes the active width and commits a keyboard change", () => {
    const { onCommit } = renderHandle();
    const handle = screen.getByRole("separator", { name: "Resize sidebar" });
    expect(handle).toHaveAttribute("aria-valuenow", "300");
    expect(handle).toHaveAttribute("aria-valuetext", "300 pixels wide");
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(onCommit).toHaveBeenCalledWith({ ...labels300, width: 320 });
  });
});
