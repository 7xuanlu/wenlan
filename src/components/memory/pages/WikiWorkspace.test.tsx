// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useEffect } from "react";
import { RECENT_PAGES_STORAGE_KEY } from "../../../lib/recentPages";
import { WikiWorkspace } from "./WikiWorkspace";
import { useWikiFolderCreation } from "./WikiFolderCreationContext";

vi.mock("./PageInventoryPanel", () => ({
  PageInventoryPanel: () => <div data-testid="wiki-directory" />,
}));

describe("WikiWorkspace", () => {
  beforeEach(() => localStorage.removeItem("wenlan:wiki-inventory:v1"));
  it("defaults to recent notes and switches modes without remounting the active editor", () => {
    const { getByTestId } = render(
      <WikiWorkspace onBrowse={vi.fn()} onCreatePage={vi.fn()} onOpenDraft={vi.fn()} onOpenPage={vi.fn()}>
        <textarea data-testid="active-editor" defaultValue="unsaved text" />
      </WikiWorkspace>,
    );
    const editor = getByTestId("active-editor");
    const mode = screen.getByRole("combobox");

    expect(mode).toHaveValue("recent");
    expect(screen.queryByTestId("wiki-directory")).not.toBeInTheDocument();
    fireEvent.change(mode, { target: { value: "folders" } });
    expect(screen.getByTestId("wiki-directory")).toBeInTheDocument();
    expect(getByTestId("active-editor")).toBe(editor);
    fireEvent.change(mode, { target: { value: "custom" } });
    expect(screen.queryByTestId("wiki-directory")).not.toBeInTheDocument();
    expect(getByTestId("active-editor")).toBe(editor);
    fireEvent.change(mode, { target: { value: "recent" } });
    expect(screen.queryByTestId("wiki-directory")).not.toBeInTheDocument();
    expect(getByTestId("active-editor")).toBe(editor);
  });

  it("restores the chosen custom mode after remount without entering the folder tree", () => {
    const props = { onBrowse: vi.fn(), onCreatePage: vi.fn(), onOpenDraft: vi.fn(), onOpenPage: vi.fn() };
    const first = render(<WikiWorkspace {...props}><textarea /></WikiWorkspace>);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "custom" } });
    first.unmount();
    render(<WikiWorkspace {...props}><textarea /></WikiWorkspace>);
    expect(screen.getByRole("combobox")).toHaveValue("custom");
    expect(screen.queryByTestId("wiki-directory")).not.toBeInTheDocument();
  });

  it("opens the mobile notes panel and registered folder form only in folders mode", async () => {
    const openForm = vi.fn();
    function RequestFolder() {
      const folderCreation = useWikiFolderCreation();
      useEffect(() => {
        if (!folderCreation) return;
        folderCreation.reportCanCreateFolder(true);
        return folderCreation.registerCreateFolderForm(openForm);
      }, [folderCreation?.reportCanCreateFolder, folderCreation?.registerCreateFolderForm]);
      return <button type="button" onClick={() => folderCreation?.requestCreateFolder()}>Request folder</button>;
    }
    render(
      <WikiWorkspace onBrowse={vi.fn()} onCreatePage={vi.fn()} onOpenDraft={vi.fn()} onOpenPage={vi.fn()}>
        <RequestFolder />
      </WikiWorkspace>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Request folder" }));
    expect(openForm).not.toHaveBeenCalled();
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "folders" } });
    fireEvent.click(screen.getByRole("button", { name: "Request folder" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Notes" })).toHaveAttribute("aria-expanded", "true"));
    expect(openForm).toHaveBeenCalledOnce();
  });

  it("reports creation folder changes to its parent", async () => {
    const onCreationFolderPathChange = vi.fn();
    function ReportPath() {
      const folderCreation = useWikiFolderCreation();
      useEffect(() => {
        folderCreation?.reportCreationFolderPath("Work/Research");
      }, [folderCreation?.reportCreationFolderPath]);
      return null;
    }
    render(
      <WikiWorkspace inventoryMode="folders" onBrowse={vi.fn()} onCreatePage={vi.fn()} onOpenDraft={vi.fn()} onOpenPage={vi.fn()} onCreationFolderPathChange={onCreationFolderPathChange}>
        <ReportPath />
      </WikiWorkspace>,
    );

    await waitFor(() => expect(onCreationFolderPathChange).toHaveBeenLastCalledWith("Work/Research"));
  });

  it("reports no folder creation path while recent notes are selected", async () => {
    const onCreationFolderPathChange = vi.fn();
    function ReportPath() {
      const folderCreation = useWikiFolderCreation();
      useEffect(() => {
        folderCreation?.reportCreationFolderPath("Work/Research");
      }, [folderCreation?.reportCreationFolderPath]);
      return null;
    }
    render(
      <WikiWorkspace onBrowse={vi.fn()} onCreatePage={vi.fn()} onOpenDraft={vi.fn()} onOpenPage={vi.fn()} onCreationFolderPathChange={onCreationFolderPathChange}>
        <ReportPath />
      </WikiWorkspace>,
    );

    await waitFor(() => expect(onCreationFolderPathChange).toHaveBeenLastCalledWith(null));
  });

  it("renders the persisted recent history in visit order and marks the active note", () => {
    localStorage.setItem(RECENT_PAGES_STORAGE_KEY, JSON.stringify({
      version: 1,
      entries: [
        { id: "newer", title: "Newer note", visitedAt: Date.now() },
        { id: "older", title: "Older note", visitedAt: Date.now() - 10 },
      ],
    }));
    const onOpenRecentPage = vi.fn();
    render(
      <WikiWorkspace currentPageId="older" onBrowse={vi.fn()} onCreatePage={vi.fn()} onOpenDraft={vi.fn()} onOpenPage={vi.fn()} onOpenRecentPage={onOpenRecentPage}>
        <textarea data-testid="active-editor" />
      </WikiWorkspace>,
    );

    const newer = screen.getByRole("button", { name: "Newer note" });
    const older = screen.getByRole("button", { name: "Older note" });
    expect(newer.compareDocumentPosition(older) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(older).toHaveAttribute("aria-current", "page");
    fireEvent.click(newer);
    expect(onOpenRecentPage).toHaveBeenCalledWith("newer");
  });

  it("closes the notes drawer after opening a recent note", () => {
    localStorage.setItem(RECENT_PAGES_STORAGE_KEY, JSON.stringify({
      version: 1,
      entries: [{ id: "recent-page", title: "Recent page", visitedAt: Date.now() }],
    }));
    const onOpenRecentPage = vi.fn();
    render(
      <WikiWorkspace onBrowse={vi.fn()} onCreatePage={vi.fn()} onOpenDraft={vi.fn()} onOpenPage={vi.fn()} onOpenRecentPage={onOpenRecentPage}>
        <textarea data-testid="active-editor" />
      </WikiWorkspace>,
    );
    const notesToggle = screen.getByRole("button", { name: "Notes" });

    fireEvent.click(notesToggle);
    expect(notesToggle).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(screen.getByRole("button", { name: "Recent page" }));
    expect(onOpenRecentPage).toHaveBeenCalledWith("recent-page");
    expect(notesToggle).toHaveAttribute("aria-expanded", "false");
  });

  it("shows the empty recent-notes state without inventing entries", () => {
    localStorage.removeItem(RECENT_PAGES_STORAGE_KEY);
    const { container } = render(
      <WikiWorkspace onBrowse={vi.fn()} onCreatePage={vi.fn()} onOpenDraft={vi.fn()} onOpenPage={vi.fn()}>
        <textarea data-testid="active-editor" />
      </WikiWorkspace>,
    );

    expect(container.querySelector(".wiki-recent-notes-empty")).toBeInTheDocument();
    expect(container.querySelector(".wiki-recent-notes-list")).not.toBeInTheDocument();
  });
});
