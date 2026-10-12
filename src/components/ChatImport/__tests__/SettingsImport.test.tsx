import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import "../../../i18n";
import SourcesSection from "../../memory/settings/sections/SourcesSection";

vi.mock("../../memory/sources/SourcesSection", () => ({ default: () => <div /> }));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

vi.mock("../../../lib/tauri", () => ({
  importChatExport: vi.fn(),
  saveTempFile: vi.fn(),
  listPendingImports: vi.fn(() => Promise.resolve([])),
  importStageLabel: (s: string) => s,
  IMPORT_STAGE_LABELS: {},
}));

import { ImportFlow } from "../ImportFlow";

describe("Settings page chat import section", () => {
  it("ImportFlow renders in idle state with DropZone", () => {
    render(<ImportFlow />);
    expect(screen.getByTestId("chat-import-drop-zone")).toBeInTheDocument();
  });

  it("ImportFlow shows the drop prompt text", () => {
    render(<ImportFlow />);
    expect(
      screen.getByText(/drop export zip here/i),
    ).toBeInTheDocument();
  });

  it("distinguishes short-memory entry from chat-history imports and shows provider export paths", () => {
    render(<SourcesSection onImport={() => {}} />);

    expect(screen.getByText("Add short memory entries: paste text or select text files, with one memory per line.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Paste or import" })).toBeInTheDocument();
    expect(screen.getByText(/Import conversations from a ChatGPT or Claude export ZIP/)).toBeInTheDocument();
    expect(screen.getByText("How to export")).toBeInTheDocument();
    expect(screen.getByText("Settings → Data controls → Export data")).toBeInTheDocument();
    expect(screen.getByText("Settings → Privacy → Export data")).toBeInTheDocument();
  });
});
