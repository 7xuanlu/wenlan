// SPDX-License-Identifier: AGPL-3.0-only
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import PageInfoDrawer from "./PageInfoDrawer";
import NoteInspectorTabs from "./NoteInspectorTabs";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => ({
      "pageInspector.label": "Note views",
      "pageInspector.info": "Info",
      "knowledgeContext.localGraph": "Connections",
      "pageCanvas.tabCanvas": "Mind map",
    }[key] ?? key),
  }),
}));

describe("NoteInspectorTabs", () => {
  it("exposes a labelled tablist with linked tabs and focus-only arrow navigation", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<NoteInspectorTabs active="info" idPrefix="note-view" onSelect={onSelect} />);

    const tablist = screen.getByRole("tablist", { name: "Note views" });
    const info = screen.getByRole("tab", { name: "Info" });
    const canvas = screen.getByRole("tab", { name: "Mind map" });
    expect(tablist).toBeInTheDocument();
    expect(info).toHaveAttribute("id", "note-view-info");
    expect(info).toHaveAttribute("aria-controls", "note-view-panel");
    expect(screen.queryByRole("tab", { name: "Connections" })).not.toBeInTheDocument();
    expect(canvas).toHaveAttribute("id", "note-view-canvas");
    expect(info).toHaveAttribute("aria-selected", "true");
    expect(info).toHaveAttribute("tabindex", "0");
    expect(canvas).toHaveAttribute("tabindex", "-1");

    await user.tab();
    expect(info).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(canvas).toHaveFocus();
    expect(info).toHaveAttribute("aria-selected", "true");
    expect(onSelect).not.toHaveBeenCalled();
    await user.keyboard("{End}");
    expect(canvas).toHaveFocus();
    expect(onSelect).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    expect(onSelect).toHaveBeenCalledExactlyOnceWith("canvas");
    expect(info).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{Home}");
    expect(info).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(canvas).toHaveFocus();
    expect(onSelect).toHaveBeenCalledOnce();
    await user.keyboard(" ");
    expect(onSelect).toHaveBeenCalledTimes(2);
    expect(onSelect).toHaveBeenLastCalledWith("canvas");
  });

  it("keeps modal tab trapping intact after roving focus moves to a negative-tabindex tab", async () => {
    const user = userEvent.setup();
    render(<PageInfoDrawer
      closeLabel="Close inspector"
      headerContent={<NoteInspectorTabs active="info" idPrefix="modal-view" onSelect={() => {}} />}
      inspector
      onClose={() => {}}
      open
      title="Note views"
    >
      <button type="button">Panel action</button>
    </PageInfoDrawer>);

    const canvas = screen.getByRole("tab", { name: "Mind map" });
    canvas.parentElement?.querySelector<HTMLButtonElement>("[aria-selected='true']")?.focus();
    await user.keyboard("{ArrowRight}");
    expect(canvas).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Close inspector" })).toHaveFocus();
  });

  it("retains the initiating tab focus while pending and rejects duplicate pointer and keyboard selection", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const { rerender } = render(<NoteInspectorTabs active="info" idPrefix="pending-view" onSelect={onSelect} />);
    const canvas = screen.getByRole("tab", { name: "Mind map" });
    canvas.focus();
    await user.keyboard("{Enter}");
    expect(onSelect).toHaveBeenCalledExactlyOnceWith("canvas");
    rerender(<NoteInspectorTabs active="info" disabled idPrefix="pending-view" onSelect={onSelect} />);
    expect(canvas).toHaveFocus();
    for (const tab of screen.getAllByRole("tab")) {
      expect(tab).toHaveAttribute("aria-disabled", "true");
      expect(tab).toHaveAttribute("aria-busy", "true");
      expect(tab).not.toBeDisabled();
    }
    await user.keyboard("{Enter} {ArrowLeft}");
    expect(canvas).toHaveFocus();
    await user.click(canvas);
    expect(onSelect).toHaveBeenCalledTimes(1);
    rerender(<NoteInspectorTabs active="canvas" idPrefix="pending-view" onSelect={onSelect} />);
    expect(canvas).toHaveFocus();
    expect(canvas).toHaveAttribute("aria-selected", "true");
    expect(canvas).toHaveAttribute("tabindex", "0");
    expect(canvas).not.toHaveAttribute("aria-busy");
  });
});
