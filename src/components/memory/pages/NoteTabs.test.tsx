// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { NoteTabs } from "./NoteTabs";
import type { NoteTab } from "./noteTabState";

const tabs: NoteTab[] = [
  { key: "note:one", view: { kind: "page", pageId: "one" }, title: "First note" },
  { key: "note:two", view: { kind: "page", pageId: "two" }, title: "Second note" },
];

function renderTabs(overrides: Partial<React.ComponentProps<typeof NoteTabs>> = {}) {
  const props = {
    tabs,
    activeKey: tabs[0].key,
    onSelect: vi.fn(),
    onClose: vi.fn(),
    onCreate: vi.fn(),
    ...overrides,
  };
  return { ...render(<NoteTabs {...props} />), props };
}

describe("NoteTabs", () => {
  it("does not show a direct move action and marks its group and tabs for drag targeting", () => {
    const onMoveToOtherGroup = vi.fn();
    renderTabs({ onMoveToOtherGroup, moveLabel: "Move to other group", groupId: "secondary" });

    expect(document.querySelector(".note-tabs")).toHaveAttribute("data-note-tab-group", "secondary");
    expect(document.querySelector('[data-note-tab-key="note:one"]')).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open beside" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Move back" })).not.toBeInTheDocument();
  });

  it("opens the selected tab's action from the keyboard and restores tab focus on Escape", async () => {
    const user = userEvent.setup();
    const { props } = renderTabs({ onMoveToOtherGroup: vi.fn(), moveLabel: "Move to other group" });
    const tab = screen.getByRole("tab", { name: "First note" });
    tab.focus();

    await user.keyboard("{Shift>}{F10}{/Shift}");

    const action = await screen.findByRole("menuitem", { name: "Move to other group" });
    expect(action).toHaveFocus();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(tab).toHaveFocus());
    expect(props.onMoveToOtherGroup).not.toHaveBeenCalled();
  });

  it("opens from the pointer context menu and moves the tab that was invoked", async () => {
    const user = userEvent.setup();
    const onMoveToOtherGroup = vi.fn();
    const { props } = renderTabs({ onMoveToOtherGroup, moveLabel: "Move to other group" });
    const tab = screen.getByRole("tab", { name: "Second note" });

    fireEvent.contextMenu(tab, { clientX: 20, clientY: 30 });
    await user.click(await screen.findByRole("menuitem", { name: "Move to other group" }));

    expect(onMoveToOtherGroup).toHaveBeenCalledOnce();
    expect(onMoveToOtherGroup).toHaveBeenCalledWith(tabs[1]);
    expect(props.onSelect).not.toHaveBeenCalled();
  });

  it("uses a unique content target and supplied tab list label", () => {
    renderTabs({ contentId: "group-two-content", tabListLabel: "Notes in second group" });

    expect(screen.getByRole("tablist", { name: "Notes in second group" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "First note" })).toHaveAttribute("aria-controls", "group-two-content");
    expect(screen.getByRole("tab", { name: "Second note" })).toHaveAttribute("aria-controls", "group-two-content");
  });

  it("retains arrow navigation, close focus, and new tab behavior", async () => {
    const user = userEvent.setup();
    const { props, rerender } = renderTabs();
    const first = screen.getByRole("tab", { name: "First note" });
    const second = screen.getByRole("tab", { name: "Second note" });

    first.focus();
    await user.keyboard("{ArrowRight}");
    expect(second).toHaveFocus();
    await user.click(screen.getByRole("button", { name: "Close First note" }));
    expect(props.onClose).toHaveBeenCalledWith(tabs[0]);

    rerender(<NoteTabs {...props} tabs={[tabs[1]]} activeKey={tabs[1].key} />);
    await waitFor(() => expect(screen.getByRole("tab", { name: "Second note" })).toHaveFocus());
    await user.click(screen.getByRole("button", { name: "New page" }));
    expect(props.onCreate).toHaveBeenCalledOnce();
  });
});
