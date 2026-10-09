// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { NoteTabDragProvider } from "./NoteTabDragProvider";

function rect(left: number, top: number, width: number, height: number) {
  return { left, top, width, height, right: left + width, bottom: top + height, x: left, y: top, toJSON: () => ({}) } as DOMRect;
}

function giveRect(element: Element, bounds: DOMRect) {
  vi.spyOn(element, "getBoundingClientRect").mockReturnValue(bounds);
  vi.spyOn(element, "getClientRects").mockReturnValue({ 0: bounds, length: 1, item: (index: number) => index === 0 ? bounds : null, [Symbol.iterator]: function* () { yield bounds; } } as DOMRectList);
}

describe("NoteTabDragProvider", () => {
  afterEach(() => { vi.restoreAllMocks(); Reflect.deleteProperty(document, "elementFromPoint"); });

  it("reports a cross-group drop at the append position after the pointer threshold", () => {
    const onDrop = vi.fn();
    const { container } = render(<>
      <div className="memory-shell">
        <section data-note-group-id="primary">
          <div className="note-tabs" data-note-tab-group="primary"><div className="note-tabs-list"><div className="note-tab" data-note-tab-key="note:one"><button role="tab">First note</button></div></div></div>
        </section>
        <section data-note-group-id="secondary">
          <div className="note-tabs" data-note-tab-group="secondary"><div className="note-tabs-list"><div className="note-tab" data-note-tab-key="note:two"><button role="tab">Second note</button></div></div></div>
        </section>
      </div>
      <NoteTabDragProvider onDrop={onDrop} />
    </>);
    const sourceStrip = container.querySelector<HTMLElement>('[data-note-tab-group="primary"]')!;
    const sourceFrame = container.querySelector<HTMLElement>('[data-note-group-id="primary"]')!;
    const sourceList = sourceStrip.querySelector<HTMLElement>(".note-tabs-list")!;
    const targetStrip = container.querySelector<HTMLElement>('[data-note-tab-group="secondary"]')!;
    const targetFrame = container.querySelector<HTMLElement>('[data-note-group-id="secondary"]')!;
    const targetList = targetStrip.querySelector<HTMLElement>(".note-tabs-list")!;
    const targetWrapper = targetStrip.querySelector<HTMLElement>("[data-note-tab-key]")!;
    const sourceButton = sourceStrip.querySelector<HTMLButtonElement>("[role=tab]")!;
    giveRect(sourceStrip, rect(0, 0, 190, 48));
    giveRect(sourceFrame, rect(0, 0, 190, 200));
    giveRect(sourceList, rect(0, 0, 160, 40));
    giveRect(targetStrip, rect(200, 0, 190, 48));
    giveRect(targetFrame, rect(200, 0, 190, 200));
    giveRect(targetList, rect(200, 0, 180, 40));
    giveRect(targetWrapper, rect(220, 4, 90, 36));
    Object.defineProperty(document, "elementFromPoint", { configurable: true, value: vi.fn((x: number, y: number) => x >= 200 && y < 48 ? targetWrapper : sourceButton) });

    fireEvent.pointerDown(sourceButton, { pointerId: 9, pointerType: "mouse", button: 0, clientX: 20, clientY: 20 });
    fireEvent.pointerMove(document, { pointerId: 9, pointerType: "mouse", clientX: 330, clientY: 20 });
    expect(document.querySelector(".note-tab-drag-ghost")).toBeInTheDocument();
    fireEvent.pointerUp(document, { pointerId: 9, pointerType: "mouse", clientX: 330, clientY: 20 });

    expect(onDrop).toHaveBeenCalledOnce();
    expect(onDrop).toHaveBeenCalledWith({ sourceGroup: "primary", targetGroup: "secondary", tabKey: "note:one", beforeKey: null });
    expect(document.querySelector(".note-tab-drag-ghost")).not.toBeInTheDocument();
  });

  it("does not call onDrop for a pointer movement below the drag threshold", () => {
    const onDrop = vi.fn();
    const { container } = render(<>
      <div className="memory-shell"><section data-note-group-id="primary"><div className="note-tabs" data-note-tab-group="primary"><div className="note-tabs-list"><div className="note-tab" data-note-tab-key="note:one"><button role="tab">First note</button></div></div></div></section></div>
      <NoteTabDragProvider onDrop={onDrop} />
    </>);
    const strip = container.querySelector<HTMLElement>(".note-tabs")!;
    const frame = container.querySelector<HTMLElement>("[data-note-group-id=primary]")!;
    const button = strip.querySelector<HTMLButtonElement>("[role=tab]")!;
    giveRect(strip, rect(0, 0, 190, 48));
    giveRect(frame, rect(0, 0, 190, 120));
    fireEvent.pointerDown(button, { pointerId: 2, pointerType: "mouse", button: 0, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(document, { pointerId: 2, pointerType: "mouse", clientX: 14, clientY: 12 });
    fireEvent.pointerUp(document, { pointerId: 2, pointerType: "mouse", clientX: 14, clientY: 12 });
    expect(onDrop).not.toHaveBeenCalled();
  });
});
