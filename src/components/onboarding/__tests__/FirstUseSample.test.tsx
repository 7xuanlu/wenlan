// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, act, within } from "@testing-library/react";
import { FirstUseSample } from "../FirstUseSample";

vi.mock("../../../lib/tauri", () => ({
  clipboardWrite: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { clipboardWrite } from "../../../lib/tauri";

const mockedInvoke = vi.mocked(invoke);
const mockedClipboardWrite = vi.mocked(clipboardWrite);

function renderSample(props = {}) {
  return render(
    <FirstUseSample
      onBackToGuide={vi.fn()}
      onBringData={vi.fn()}
      onConnect={vi.fn()}
      {...props}
    />,
  );
}

function skipToResult() {
  fireEvent.click(screen.getByRole("button", { name: "Skip to result" }));
}

function openAiPanel() {
  skipToResult();
  fireEvent.click(screen.getByRole("button", { name: "Use with AI" }));
}

function stubMatchMedia(initial: boolean) {
  const original = window.matchMedia;
  let matches = initial;
  const listeners = new Set<(event: { matches: boolean }) => void>();
  const mq = {
    get matches() {
      return matches;
    },
    media: "(prefers-reduced-motion: reduce)",
    addEventListener: vi.fn((_type: string, cb: (event: { matches: boolean }) => void) => {
      listeners.add(cb);
    }),
    removeEventListener: vi.fn((_type: string, cb: (event: { matches: boolean }) => void) => {
      listeners.delete(cb);
    }),
  };
  window.matchMedia = vi.fn().mockReturnValue(mq);
  return {
    mq,
    setMatches(next: boolean) {
      matches = next;
      listeners.forEach((cb) => cb({ matches: next }));
    },
    restore() {
      window.matchMedia = original;
    },
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mockedClipboardWrite.mockResolvedValue(undefined);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("FirstUseSample demo isolation", () => {
  it("issues no daemon calls while walking the whole sample", async () => {
    renderSample();
    skipToResult();
    expect(
      screen.getByText("An unhurried Tainan weekend"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("sample-citation-sample-cite-2"));
    expect(screen.getByTestId("sample-citation-dialog")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    fireEvent.click(screen.getByRole("button", { name: "Open related page" }));
    expect(screen.getByTestId("sample-related-dialog")).toBeInTheDocument();
    // The only daemon-adjacent call allowed on the sample path is clipboard.
    expect(mockedInvoke).not.toHaveBeenCalled();
  });

  it("autoplays through phases and cleans its timer up on unmount", async () => {
    vi.useFakeTimers();
    const clearSpy = vi.spyOn(window, "clearTimeout");
    const { unmount } = renderSample();
    expect(screen.getByTestId("sample-phase-0")).toHaveAttribute(
      "data-active",
      "true",
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });
    expect(screen.getByTestId("sample-phase-1")).toHaveAttribute(
      "data-active",
      "true",
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(3000);
    });
    expect(screen.getByTestId("sample-phase-3")).toHaveAttribute(
      "data-active",
      "true",
    );
    unmount();
    expect(clearSpy).toHaveBeenCalled();
    clearSpy.mockRestore();
  });

  it("stays manual under reduced motion", async () => {
    const media = stubMatchMedia(true);
    try {
      vi.useFakeTimers();
      renderSample();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(9000);
      });
      expect(screen.getByTestId("sample-phase-0")).toHaveAttribute(
        "data-active",
        "true",
      );
    } finally {
      media.restore();
    }
  });
});

describe("FirstUseSample source rack and page flow", () => {
  it("keeps the three-source rack visible from import through the final page", () => {
    renderSample();
    for (const testId of [
      "sample-source-sample-memory",
      "sample-source-sample-file",
      "sample-source-sample-page",
    ]) {
      expect(screen.getByTestId(testId)).toBeInTheDocument();
    }
    skipToResult();
    for (const testId of [
      "sample-source-sample-memory",
      "sample-source-sample-file",
      "sample-source-sample-page",
    ]) {
      expect(screen.getByTestId(testId)).toBeInTheDocument();
    }
    expect(
      screen.getByText("Weekend trip decision"),
    ).toBeInTheDocument();
    expect(screen.getByText("packing-list.md")).toBeInTheDocument();
    expect(screen.getByText("Slow travel")).toBeInTheDocument();
  });

  it("opens the file original with path and line context", () => {
    renderSample();
    skipToResult();
    fireEvent.click(screen.getByTestId("sample-citation-sample-cite-2"));
    const dialog = screen.getByTestId("sample-citation-dialog");
    expect(dialog).toHaveAttribute("data-source-type", "file");
    expect(
      within(dialog).getByText("File trip folder / packing-list.md · line 2"),
    ).toBeInTheDocument();
    // The quote appears twice: once as the cited excerpt, once highlighted
    // in the file original.
    expect(
      within(dialog).getAllByText("Book a stay near the Tainan station and pack light."),
    ).toHaveLength(2);
  });

  it("opens the captured-memory original without file context", () => {
    renderSample();
    skipToResult();
    fireEvent.click(screen.getByTestId("sample-citation-sample-cite-1"));
    const dialog = screen.getByTestId("sample-citation-dialog");
    expect(dialog).toHaveAttribute("data-source-type", "memory");
    expect(
      within(dialog).getByText("Captured from conversation"),
    ).toBeInTheDocument();
    expect(
      within(dialog).queryByText(/File /),
    ).not.toBeInTheDocument();
  });

  it("opens the related internal sample page and returns", () => {
    renderSample();
    skipToResult();
    fireEvent.click(screen.getByRole("button", { name: "Open related page" }));
    const dialog = screen.getByTestId("sample-related-dialog");
    expect(
      within(dialog).getByText("Two things left before departure"),
    ).toBeInTheDocument();
    fireEvent.click(
      within(dialog).getByRole("button", { name: "Back to the page" }),
    );
    expect(
      screen.queryByTestId("sample-related-dialog"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("An unhurried Tainan weekend"),
    ).toBeInTheDocument();
  });

  it("reveals the AI step only through Use with AI, with a way back", () => {
    renderSample();
    skipToResult();
    expect(
      screen.queryByText("Let AI use your knowledge"),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Use with AI" }));
    expect(
      screen.getByText("Let AI use your knowledge"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Back to the page" }));
    expect(
      screen.queryByText("Let AI use your knowledge"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("An unhurried Tainan weekend"),
    ).toBeInTheDocument();
  });
});

describe("FirstUseSample dialogs", () => {
  it("focuses the dialog on open, closes on Escape, and restores trigger focus", () => {
    renderSample();
    skipToResult();
    const trigger = screen.getByTestId("sample-citation-sample-cite-1");
    // Browsers focus a button on mousedown; jsdom needs the explicit focus.
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = screen.getByTestId("sample-citation-dialog");
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(screen.getByTestId("first-use-sample")).toHaveAttribute("inert");
    fireEvent.keyDown(document, { key: "Escape" });
    expect(
      screen.queryByTestId("sample-citation-dialog"),
    ).not.toBeInTheDocument();
    expect(
      screen.getByTestId("first-use-sample"),
    ).not.toHaveAttribute("inert");
    expect(document.activeElement).toBe(trigger);
  });

  it("traps Tab and Shift+Tab inside the dialog", () => {
    renderSample();
    skipToResult();
    fireEvent.click(screen.getByRole("button", { name: "Open related page" }));
    const dialog = screen.getByTestId("sample-related-dialog");
    const closeButton = within(dialog).getByRole("button", { name: "Close" });
    const backButton = within(dialog).getByRole("button", {
      name: "Back to the page",
    });
    backButton.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(document.activeElement).toBe(closeButton);
    closeButton.focus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(backButton);
  });
});

describe("FirstUseSample AI-use panel", () => {
  it("suggests own-data recall: @wenlan for ChatGPT, slash commands otherwise", () => {
    renderSample();
    openAiPanel();
    expect(
      screen.getByText(/@wenlan recall the decisions/),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "Codex" }));
    expect(
      screen.getByText("/recall decisions and next steps from my imports"),
    ).toBeInTheDocument();
    expect(screen.getByText("/handoff")).toBeInTheDocument();
    expect(screen.getByText("/brief")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("tab", { name: "Claude" }));
    expect(
      screen.getByText("/recall decisions and next steps from my imports"),
    ).toBeInTheDocument();
  });

  it("reports a clipboard copy as copied", async () => {
    renderSample();
    openAiPanel();
    // The recall row lives in the tabpanel; the handoff/brief icon buttons
    // share the same accessible name outside it.
    const panel = screen.getByRole("tabpanel");
    fireEvent.click(
      within(panel).getByRole("button", { name: "Copy command" }),
    );
    expect(mockedClipboardWrite).toHaveBeenCalledTimes(1);
    expect(await within(panel).findByText("Copied")).toBeInTheDocument();
  });

  it("reports a clipboard rejection as failure, never as success", async () => {
    mockedClipboardWrite.mockRejectedValueOnce(new Error("denied"));
    renderSample();
    openAiPanel();
    const panel = screen.getByRole("tabpanel");
    fireEvent.click(
      within(panel).getByRole("button", { name: "Copy command" }),
    );
    expect(await screen.findByText(/Copy failed/)).toBeInTheDocument();
    expect(screen.queryByText("Copied")).not.toBeInTheDocument();
  });

  it("ignores stale copy resolutions that settle out of order", async () => {
    const resolvers: Array<() => void> = [];
    mockedClipboardWrite.mockImplementation(
      () => new Promise<void>((resolve) => resolvers.push(resolve)),
    );
    renderSample();
    openAiPanel();
    const panel = screen.getByRole("tabpanel");
    fireEvent.click(
      within(panel).getByRole("button", { name: "Copy command" }),
    );
    const copyButtons = screen.getAllByRole("button", { name: "Copy command" });
    fireEvent.click(copyButtons[1]);
    await act(async () => {
      resolvers[1]!();
    });
    const handoffRow = screen.getByText(/Leave progress behind/).closest("li")!;
    expect(within(handoffRow).getByTestId("copy-ok")).toBeInTheDocument();
    await act(async () => {
      resolvers[0]!();
    });
    // The stale recall resolution changes nothing: no recall success, no error.
    expect(within(panel).queryByTestId("copy-ok")).not.toBeInTheDocument();
    expect(screen.queryByText(/Copy failed/)).not.toBeInTheDocument();
    expect(within(handoffRow).getByTestId("copy-ok")).toBeInTheDocument();
  });

  it("drops a pending copy when the client switches before it settles", async () => {
    const resolvers: Array<() => void> = [];
    mockedClipboardWrite.mockImplementation(
      () => new Promise<void>((resolve) => resolvers.push(resolve)),
    );
    renderSample();
    openAiPanel();
    const panel = screen.getByRole("tabpanel");
    fireEvent.click(
      within(panel).getByRole("button", { name: "Copy command" }),
    );
    fireEvent.click(screen.getByRole("tab", { name: "Codex" }));
    await act(async () => {
      resolvers[0]!();
    });
    expect(mockedClipboardWrite).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("Copied")).not.toBeInTheDocument();
    expect(screen.queryByText(/Copy failed/)).not.toBeInTheDocument();
  });

  it("routes CTAs to the genuine setup route and the own-data chooser", () => {
    const onConnect = vi.fn();
    const onBringData = vi.fn();
    renderSample({ onConnect, onBringData });
    openAiPanel();
    fireEvent.click(screen.getByRole("button", { name: "Connect my AI" }));
    fireEvent.click(screen.getByRole("button", { name: "Bring my data" }));
    expect(onConnect).toHaveBeenCalledTimes(1);
    expect(onBringData).toHaveBeenCalledTimes(1);
  });
});

describe("FirstUseSample reduced motion", () => {
  it("offers no Play control but keeps manual steps working", () => {
    const media = stubMatchMedia(true);
    try {
      renderSample();
      expect(
        screen.queryByRole("button", { name: "Play" }),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "Pause" }),
      ).not.toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "Next step" }));
      expect(screen.getByTestId("sample-phase-1")).toHaveAttribute(
        "data-active",
        "true",
      );
      fireEvent.click(screen.getByRole("button", { name: "Replay" }));
      expect(screen.getByTestId("sample-phase-0")).toHaveAttribute(
        "data-active",
        "true",
      );
    } finally {
      media.restore();
    }
  });

  it("hides Play on the final phase where replay suffices", () => {
    renderSample();
    skipToResult();
    expect(
      screen.queryByRole("button", { name: "Play" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Replay" }),
    ).toBeInTheDocument();
  });

  it("follows media preference changes and cleans up its listener", () => {
    const media = stubMatchMedia(false);
    try {
      const { unmount } = renderSample();
      expect(
        screen.getByRole("button", { name: "Pause" }),
      ).toBeInTheDocument();
      act(() => {
        media.setMatches(true);
      });
      expect(
        screen.queryByRole("button", { name: "Pause" }),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByRole("button", { name: "Play" }),
      ).not.toBeInTheDocument();
      unmount();
      expect(media.mq.removeEventListener).toHaveBeenCalledWith(
        "change",
        expect.any(Function),
      );
    } finally {
      media.restore();
    }
  });
});
