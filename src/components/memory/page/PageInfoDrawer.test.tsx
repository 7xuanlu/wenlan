// SPDX-License-Identifier: AGPL-3.0-only
import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import PageInfoDrawer from "./PageInfoDrawer";

function Harness() {
  const [open, setOpen] = useState(false);
  return <>
    <button onClick={() => setOpen(true)} type="button">Page information</button>
    <PageInfoDrawer closeLabel="Close information" onClose={() => setOpen(false)} open={open} title="Page information">
      <button disabled tabIndex={0} type="button">Unavailable source</button>
      <button hidden type="button">Hidden source</button>
      <a href="#source">Open source</a>
    </PageInfoDrawer>
  </>;
}

describe("PageInfoDrawer", () => {
  it("does not mount closed content and portals the labelled modal to body when opened", async () => {
    const user = userEvent.setup();
    const { container } = render(<Harness />);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByText("Open source")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Page information" }));
    const drawer = screen.getByRole("dialog", { name: "Page information" });
    expect(drawer).toHaveAttribute("aria-modal", "true");
    expect(container.contains(drawer)).toBe(false);
    expect(screen.getByRole("button", { name: "Close information" })).toHaveFocus();
  });

  it("traps Tab around enabled visible controls and restores the trigger on Escape", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "Page information" });
    await user.click(trigger);
    const close = screen.getByRole("button", { name: "Close information" });
    await user.tab({ shift: true });
    expect(screen.getByRole("link", { name: "Open source" })).toHaveFocus();
    await user.tab();
    expect(close).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("closes on the backdrop while clicks inside stay open", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "Page information" }));
    const dialog = screen.getByRole("dialog");
    await user.click(screen.getByRole("heading", { name: "Page information" }));
    expect(dialog).toBeInTheDocument();
    fireEvent.click(dialog.parentElement!);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("contains programmatic background focus and leaves composing Escape alone", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "Page information" });
    await user.click(trigger);
    trigger.focus();
    const close = screen.getByRole("button", { name: "Close information" });
    expect(close).toHaveFocus();
    fireEvent.keyDown(close, { key: "Escape", isComposing: true });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("keeps focus and scroll position stable when callback identity changes", async () => {
    const trigger = document.createElement("button");
    document.body.append(trigger);
    trigger.focus();
    const firstClose = vi.fn();
    const nextClose = vi.fn();
    const props = { open: true, title: "Information", closeLabel: "Close", children: <a href="#source">Source</a> };
    const { rerender, unmount } = render(<PageInfoDrawer {...props} onClose={firstClose} />);
    const source = screen.getByRole("link", { name: "Source" });
    source.focus();
    document.documentElement.scrollTop = 123;
    rerender(<PageInfoDrawer {...props} onClose={nextClose} />);
    expect(source).toHaveFocus();
    expect(document.documentElement.scrollTop).toBe(123);
    fireEvent.keyDown(source, { key: "Escape" });
    expect(nextClose).toHaveBeenCalledOnce();
    expect(firstClose).not.toHaveBeenCalled();
    unmount();
    expect(trigger).toHaveFocus();
    trigger.remove();
    document.documentElement.scrollTop = 0;
  });
});
