// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import SourcesSection from "./SourcesSection";
import { open as openExternal } from "@tauri-apps/plugin-shell";

vi.mock("../../sources/SourcesSection", () => ({ default: () => <div /> }));
vi.mock("../../../ChatImport/ImportFlow", () => ({ ImportFlow: () => <div /> }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn().mockResolvedValue(undefined) }));

// R4.2/S5: `onImport` used to be optional, and the Import Memories button
// only rendered when a handler was passed in. It's required now — this
// proves the button is unconditional, not just that the type changed.
describe("SourcesSection import button", () => {
  it("renders the Import Memories button unconditionally", () => {
    render(<SourcesSection onImport={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Paste or import" })).toBeInTheDocument();
  });

  it("shows aligned export steps and links to official ChatGPT and Claude help", () => {
    render(<SourcesSection onImport={vi.fn()} />);

    expect(screen.getByText("How to export")).toBeInTheDocument();
    expect(screen.getByText("Settings → Data controls → Export data")).toBeInTheDocument();
    expect(screen.getByText("Settings → Privacy → Export data")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "ChatGPT export help" })).toHaveAttribute(
      "href",
      "https://help.openai.com/en/articles/7260999-exporting-your-chatgpt-history-and-data",
    );
    expect(screen.getByRole("link", { name: "Claude export help" })).toHaveAttribute(
      "href",
      "https://support.claude.com/en/articles/9450526-export-your-claude-data",
    );

    fireEvent.click(screen.getByRole("link", { name: "ChatGPT export help" }));
    expect(openExternal).toHaveBeenCalledWith(
      "https://help.openai.com/en/articles/7260999-exporting-your-chatgpt-history-and-data",
    );

    fireEvent.click(screen.getByRole("button", { name: "Details: How to export" }));
    expect(screen.getByRole("tooltip")).toHaveTextContent(/up to 7 days/i);
    expect(screen.getByRole("tooltip")).toHaveTextContent(/manifest JSON/i);
    expect(screen.getByRole("tooltip")).toHaveTextContent(/paired user and assistant text/i);
  });
});
