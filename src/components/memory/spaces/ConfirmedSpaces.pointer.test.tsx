import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConfirmedSpaces } from "./ConfirmedSpaces";
import { labels, makeSpace } from "./SpacesOverview.testUtils";

const work = makeSpace({ id: "work", name: "Work", sort_order: 0 });
const personal = makeSpace({ id: "personal", name: "Personal", sort_order: 1 });
const spaces = [work, personal] as const;

function renderConfirmed(onReorder = vi.fn(), pendingIds: readonly string[] = []) {
  const result = render(
    <ConfirmedSpaces
      spaces={spaces}
      allSpaces={spaces}
      labels={labels}
      filter=""
      noResults={false}
      pageCounts={new Map()}
      pendingIds={pendingIds}
      lens="rows"
      onSelect={() => undefined}
      onStar={() => undefined}
      onRename={async () => true}
      onReorder={onReorder}
      onDelete={() => undefined}
    />,
  );
  return { ...result, onReorder };
}

function rect(top: number, height = 64): DOMRect {
  return { x: 0, y: top, top, left: 0, right: 400, bottom: top + height, width: 400, height, toJSON: () => ({}) } as DOMRect;
}

function setGeometry() {
  const rows = [screen.getByTestId("space-row-work").parentElement!, screen.getByTestId("space-row-personal").parentElement!];
  vi.spyOn(rows[0], "getBoundingClientRect").mockReturnValue(rect(0));
  vi.spyOn(rows[1], "getBoundingClientRect").mockReturnValue(rect(70));
  vi.spyOn(rows[0].parentElement!, "getBoundingClientRect").mockReturnValue(rect(0, 134));
  return rows;
}

function begin(pointerId: number, clientY = 20) {
  const handle = screen.getByRole("button", { name: labels.dragSpace("Work") });
  fireEvent.pointerDown(handle, { pointerId, pointerType: "mouse", button: 0, isPrimary: true, clientY });
  return handle;
}

afterEach(() => vi.restoreAllMocks());

describe("ConfirmedSpaces pointer drag lifecycle", () => {
  it("does not reorder for a click or movement within the four-pixel threshold", () => {
    const { onReorder } = renderConfirmed();
    setGeometry();
    begin(1);
    fireEvent.pointerMove(window, { pointerId: 1, clientY: 24 });
    fireEvent.pointerUp(window, { pointerId: 1, clientY: 24 });
    expect(onReorder).not.toHaveBeenCalled();
  });

  it("keeps a just-started drag in the source slot without triggering a false reorder", () => {
    const { onReorder } = renderConfirmed();
    const rows = setGeometry();
    begin(8);
    fireEvent.pointerMove(window, { pointerId: 8, clientX: 10, clientY: 25 });
    expect(rows[0]).toHaveStyle({ transform: "translateY(5px)" });
    fireEvent.pointerUp(window, { pointerId: 8, clientX: 10, clientY: 25 });
    expect(onReorder).not.toHaveBeenCalled();
  });

  it("uses release coordinates to choose the current destination and persists once", () => {
    const { onReorder } = renderConfirmed();
    const rows = setGeometry();
    begin(2);
    fireEvent.pointerMove(window, { pointerId: 2, clientY: 30 });
    expect(rows[0]).toHaveClass("is-dragging");
    expect(rows[0]).toHaveStyle({ transform: "translateY(10px)" });
    fireEvent.pointerUp(window, { pointerId: 2, clientY: 100 });
    fireEvent.pointerUp(window, { pointerId: 2, clientY: 100 });
    expect(onReorder).toHaveBeenCalledTimes(1);
    expect(onReorder).toHaveBeenCalledWith(work, personal);
  });

  it("places an upward drag before the row at its exact midpoint", () => {
    const { onReorder } = renderConfirmed();
    setGeometry();
    const handle = screen.getByRole("button", { name: labels.dragSpace("Personal") });
    fireEvent.pointerDown(handle, { pointerId: 12, button: 0, isPrimary: true, clientX: 10, clientY: 102 });
    fireEvent.pointerMove(window, { pointerId: 12, clientX: 10, clientY: 32 });
    fireEvent.pointerUp(window, { pointerId: 12, clientX: 10, clientY: 32 });
    expect(onReorder).toHaveBeenCalledExactlyOnceWith(personal, work);
  });

  it("holds the projected drop while the write is pending and returns to canonical order on failure", async () => {
    let resolveReorder: (ok: boolean) => void = () => undefined;
    const onReorder = vi.fn(() => new Promise<boolean>((resolve) => { resolveReorder = resolve; }));
    const { onReorder: reorder } = renderConfirmed(onReorder);
    const rows = setGeometry();
    begin(9);
    fireEvent.pointerMove(window, { pointerId: 9, clientX: 10, clientY: 30 });
    fireEvent.pointerUp(window, { pointerId: 9, clientX: 10, clientY: 100 });
    expect(reorder).toHaveBeenCalledOnce();
    expect(rows[0]).toHaveStyle({ transform: "translateY(70px)" });
    expect(rows[1]).toHaveStyle({ transform: "translateY(-64px)" });
    resolveReorder(false);
    await waitFor(() => {
      expect(rows[0]).not.toHaveStyle({ transform: "translateY(70px)" });
      expect(rows[1]).not.toHaveStyle({ transform: "translateY(-64px)" });
    });
  });

  it("ignores non-primary and secondary pointer starts", () => {
    const { onReorder } = renderConfirmed();
    setGeometry();
    const handle = screen.getByRole("button", { name: labels.dragSpace("Work") });
    fireEvent.pointerDown(handle, { pointerId: 3, button: 2, isPrimary: true, clientY: 20 });
    fireEvent.pointerMove(window, { pointerId: 3, clientY: 100 });
    fireEvent.pointerUp(window, { pointerId: 3, clientY: 100 });
    fireEvent.pointerDown(handle, { pointerId: 4, button: 0, isPrimary: false, clientY: 20 });
    fireEvent.pointerMove(window, { pointerId: 4, clientY: 100 });
    fireEvent.pointerUp(window, { pointerId: 4, clientY: 100 });
    expect(onReorder).not.toHaveBeenCalled();
  });

  it("cancels when the initiating pointer is cancelled", () => {
    const { onReorder } = renderConfirmed();
    setGeometry();
    begin(5);
    fireEvent.pointerMove(window, { pointerId: 5, clientY: 90 });
    fireEvent.pointerCancel(window, { pointerId: 5 });
    fireEvent.pointerUp(window, { pointerId: 5, clientY: 100 });
    expect(onReorder).not.toHaveBeenCalled();
  });

  it.each([
    ["Escape", () => fireEvent.keyDown(window, { key: "Escape" })],
    ["window blur", () => fireEvent.blur(window)],
    ["resize", () => fireEvent.resize(window)],
    ["scroll", () => fireEvent.scroll(window)],
  ])("cancels on %s", (_name, cancel) => {
    const { onReorder } = renderConfirmed();
    setGeometry();
    begin(6);
    fireEvent.pointerMove(window, { pointerId: 6, clientY: 90 });
    cancel();
    fireEvent.pointerUp(window, { pointerId: 6, clientY: 100 });
    expect(onReorder).not.toHaveBeenCalled();
  });

  it("ignores another pointer release until the initiating pointer releases", () => {
    const { onReorder } = renderConfirmed();
    setGeometry();
    begin(10);
    fireEvent.pointerMove(window, { pointerId: 10, clientX: 10, clientY: 30 });
    fireEvent.pointerUp(window, { pointerId: 11, clientX: 10, clientY: 100 });
    expect(onReorder).not.toHaveBeenCalled();
    fireEvent.pointerUp(window, { pointerId: 10, clientX: 10, clientY: 100 });
    expect(onReorder).toHaveBeenCalledOnce();
  });

  it("cancels if the visible list changes while a drag is active", () => {
    const { onReorder, rerender } = renderConfirmed();
    setGeometry();
    begin(11);
    fireEvent.pointerMove(window, { pointerId: 11, clientY: 90 });
    rerender(
      <ConfirmedSpaces
        spaces={spaces} allSpaces={spaces} labels={labels} filter="Work" noResults={false}
        pageCounts={new Map()} pendingIds={[]} lens="rows" onSelect={() => undefined}
        onStar={() => undefined} onRename={async () => true} onReorder={onReorder} onDelete={() => undefined}
      />,
    );
    fireEvent.pointerUp(window, { pointerId: 11, clientX: 10, clientY: 100 });
    expect(onReorder).not.toHaveBeenCalled();
  });

  it("cancels an outside release and prevents dragging during a pending mutation", () => {
    const { onReorder } = renderConfirmed(vi.fn(), ["other-space"]);
    setGeometry();
    begin(7);
    fireEvent.pointerMove(window, { pointerId: 7, clientY: 90 });
    fireEvent.pointerUp(window, { pointerId: 7, clientY: 200 });
    expect(onReorder).not.toHaveBeenCalled();
  });

  it("keeps keyboard reorder available through the existing move callback", () => {
    const { onReorder } = renderConfirmed();
    fireEvent.keyDown(screen.getByRole("button", { name: labels.dragSpace("Work") }), { key: "ArrowDown" });
    expect(onReorder).toHaveBeenCalledWith(work, personal);
  });
});
