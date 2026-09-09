import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { open } from "@tauri-apps/plugin-dialog";
import { DropZone } from "../DropZone";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

describe("DropZone", () => {
  it("shows the idle prompt initially", () => {
    render(<DropZone onFileSelected={() => {}} />);
    expect(
      screen.getByText(/drop export zip here/i),
    ).toBeInTheDocument();
  });

  it("marks the decorative upload icon aria-hidden", () => {
    const { container } = render(<DropZone onFileSelected={() => {}} />);
    const icon = container.querySelector("svg");
    expect(icon).not.toBeNull();
    expect(icon).toHaveAttribute("aria-hidden", "true");
  });

  it("calls onFileSelected when a zip file is dropped", () => {
    const onFileSelected = vi.fn();
    render(<DropZone onFileSelected={onFileSelected} />);
    const dropZone = screen.getByTestId("chat-import-drop-zone");
    const file = new File(["zip bytes"], "export.zip", {
      type: "application/zip",
    });
    const dataTransfer = {
      files: [file],
      items: [],
      types: ["Files"],
    };
    fireEvent.drop(dropZone, { dataTransfer });
    expect(onFileSelected).toHaveBeenCalledWith(file);
  });

  it("rejects non-zip files on drop and shows an error", () => {
    const onFileSelected = vi.fn();
    render(<DropZone onFileSelected={onFileSelected} />);
    const dropZone = screen.getByTestId("chat-import-drop-zone");
    const file = new File(["not a zip"], "notes.txt", {
      type: "text/plain",
    });
    const dataTransfer = { files: [file], items: [], types: ["Files"] };
    fireEvent.drop(dropZone, { dataTransfer });
    expect(onFileSelected).not.toHaveBeenCalled();
    expect(screen.getByText(/must be a \.zip file/i)).toBeInTheDocument();
  });

  it("calls onPathSelected when a file is picked via native dialog", async () => {
    const mockOpen = open as ReturnType<typeof vi.fn>;
    mockOpen.mockResolvedValue("/tmp/export.zip");

    const onPathSelected = vi.fn();
    render(<DropZone onFileSelected={() => {}} onPathSelected={onPathSelected} />);

    const button = screen.getByRole("button", { name: /choose file/i });
    await button.click();

    // Wait for async dialog
    await vi.waitFor(() => {
      expect(onPathSelected).toHaveBeenCalledWith("/tmp/export.zip");
    });
  });

  it("does nothing when user cancels the native dialog", async () => {
    const mockOpen = open as ReturnType<typeof vi.fn>;
    mockOpen.mockResolvedValue(null);

    const onPathSelected = vi.fn();
    render(<DropZone onFileSelected={() => {}} onPathSelected={onPathSelected} />);

    const button = screen.getByRole("button", { name: /choose file/i });
    await button.click();

    // Small delay to ensure async resolves
    await new Promise((r) => setTimeout(r, 50));
    expect(onPathSelected).not.toHaveBeenCalled();
  });

  it("blocks file selection while disabled", async () => {
    const onFileSelected = vi.fn();
    const onPathSelected = vi.fn();
    const mockOpen = open as ReturnType<typeof vi.fn>;
    render(
      <DropZone
        onFileSelected={onFileSelected}
        onPathSelected={onPathSelected}
        disabled
      />,
    );

    const dropZone = screen.getByTestId("chat-import-drop-zone");
    const file = new File(["zip bytes"], "export.zip", { type: "application/zip" });
    fireEvent.drop(dropZone, {
      dataTransfer: { files: [file], items: [], types: ["Files"] },
    });
    await screen.getByRole("button", { name: /choose file/i }).click();

    expect(onFileSelected).not.toHaveBeenCalled();
    expect(onPathSelected).not.toHaveBeenCalled();
    expect(mockOpen).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: /choose file/i })).toBeDisabled();
  });
});
