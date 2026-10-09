// SPDX-License-Identifier: AGPL-3.0-only
import { useContext } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  WorkspaceDocumentToolsHostContext,
  WorkspaceNoteGroupContext,
  WorkspacePaneHostContext,
} from "../navigation/WorkspacePaneHost";
import { NoteGroupFrame } from "./NoteGroupFrame";

function ContextProbe() {
  const group = useContext(WorkspaceNoteGroupContext);
  const paneHost = useContext(WorkspacePaneHostContext);
  const toolsHost = useContext(WorkspaceDocumentToolsHostContext);
  return <output data-testid="context-probe" data-group={group?.id} data-element={group?.element?.className}
    data-pane-host={paneHost?.className} data-tools-host={toolsHost?.className} />;
}

describe("NoteGroupFrame", () => {
  it("provides group-local inspector and document-tool hosts around wiki content", () => {
    render(<NoteGroupFrame id="left" label="Left note group" tabs={<div>Tabs</div>} contentId="left-note">
      <ContextProbe />
    </NoteGroupFrame>);

    const content = document.getElementById("left-note")!;
    const frame = document.querySelector<HTMLElement>(".note-group-frame")!;
    const probe = screen.getByTestId("context-probe");
    const inspectorHost = frame.querySelector<HTMLElement>(".note-group-inspector-host")!;
    const toolsHost = frame.querySelector<HTMLElement>(".note-group-document-tools")!;
    expect(content).toHaveAttribute("id", "left-note");
    expect(content).toHaveClass("wiki-workspace-content");
    expect(probe).toHaveAttribute("data-group", "left");
    expect(probe).toHaveAttribute("data-element", "note-group-frame");
    expect(probe).toHaveAttribute("data-pane-host", "note-group-inspector-host");
    expect(probe).toHaveAttribute("data-tools-host", "note-group-document-tools");
    expect(frame.querySelector(".note-group-document")).toContainElement(content);
    expect(frame).toContainElement(inspectorHost);
    expect(toolsHost).toHaveClass("note-group-document-tools");
  });

  it("reports focus and pointer interaction for the active group", () => {
    const onFocus = vi.fn();
    const { container } = render(<NoteGroupFrame id="right" label="Right note group" tabs={<button>Tab</button>} onFocus={onFocus}>
      <button>Document</button>
    </NoteGroupFrame>);
    const frame = container.querySelector<HTMLElement>(".note-group-frame")!;
    fireEvent.focus(screen.getByRole("button", { name: "Document" }));
    fireEvent.pointerDown(frame, { button: 0 });
    expect(onFocus).toHaveBeenCalledTimes(2);
  });
});
