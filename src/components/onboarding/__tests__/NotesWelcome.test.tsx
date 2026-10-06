// SPDX-License-Identifier: AGPL-3.0-only
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../../i18n";
import { NotesWelcome } from "../NotesWelcome";
import { clipboardWrite } from "../../../lib/tauri";

vi.mock("../../../lib/tauri", () => ({ clipboardWrite: vi.fn() }));

function deferred() {
  let resolve!: () => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<void>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function openButton() {
  return screen.getByRole("button", { name: i18n.t("setup.notesFirst.open") });
}

function sampleButton() {
  return screen.getByRole("button", { name: i18n.t("firstUse.guide.tryExample") });
}

function enterSampleAi() {
  fireEvent.click(sampleButton());
  fireEvent.click(screen.getByRole("button", { name: i18n.t("firstUse.sample.skipToResult") }));
  fireEvent.click(screen.getByRole("button", { name: i18n.t("firstUse.sample.useWithAi") }));
}

describe("NotesWelcome", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("offers the primary notes action and an optional sample without setup controls or runtime calls", () => {
    render(<NotesWelcome onComplete={vi.fn()} />);

    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(i18n.t("setup.welcomeTitle"));
    expect(screen.getByText(i18n.t("setup.notesFirst.body"))).toBeInTheDocument();
    expect(screen.getByText(i18n.t("setup.notesFirst.hint"))).toBeInTheDocument();
    expect(screen.getAllByRole("button")).toEqual([openButton(), sampleButton()]);
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("awaits completion and keeps the action disabled while it is pending", async () => {
    const completion = deferred();
    const onComplete = vi.fn(() => completion.promise);
    render(<NotesWelcome onComplete={onComplete} />);

    fireEvent.click(openButton());
    expect(onComplete).toHaveBeenCalledTimes(1);
    expect(onComplete).toHaveBeenCalledWith();
    expect(openButton()).toBeDisabled();
    expect(sampleButton()).toBeDisabled();
    expect(openButton()).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();

    await act(async () => completion.resolve());

    expect(openButton()).toBeEnabled();
    expect(openButton()).toHaveAttribute("aria-busy", "false");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("guards clicks delivered in the same batch before pending state renders", async () => {
    const completion = deferred();
    const onComplete = vi.fn(() => completion.promise);
    render(<NotesWelcome onComplete={onComplete} />);
    const button = openButton();

    act(() => {
      button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(onComplete).toHaveBeenCalledTimes(1);
    expect(button).toBeDisabled();
    await act(async () => completion.resolve());
  });

  it("shows a failure, then allows a retry and clears the alert while retrying", async () => {
    const first = deferred();
    const retry = deferred();
    const onComplete = vi.fn()
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => retry.promise);
    render(<NotesWelcome onComplete={onComplete} />);

    fireEvent.click(openButton());
    await act(async () => first.reject(new Error("could not save completion")));

    expect(screen.getByRole("alert")).toHaveTextContent(i18n.t("setup.notesFirst.openFailed"));
    expect(openButton()).toBeEnabled();
    expect(screen.getByRole("heading", { level: 1 })).toBeInTheDocument();

    fireEvent.click(openButton());
    expect(onComplete).toHaveBeenCalledTimes(2);
    expect(openButton()).toBeDisabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await act(async () => retry.resolve());
    expect(openButton()).toBeEnabled();
  });

  it("handles synchronous callbacks and synchronous failures", async () => {
    const onComplete = vi.fn()
      .mockImplementationOnce(() => { throw new Error("could not save completion"); })
      .mockImplementationOnce(() => undefined);
    render(<NotesWelcome onComplete={onComplete} />);

    fireEvent.click(openButton());
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(openButton()).toBeEnabled();

    fireEvent.click(openButton());
    await waitFor(() => expect(openButton()).toBeEnabled());
    expect(onComplete).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("opens the actual sample directly and returns focus without completing or calling the runtime", () => {
    const onComplete = vi.fn();
    render(<NotesWelcome onComplete={onComplete} />);
    sampleButton().focus();
    fireEvent.click(sampleButton());

    expect(screen.getByTestId("first-use-sample")).toBeInTheDocument();
    expect(screen.getByTestId("sample-phase-0")).toHaveAttribute("data-active", "true");
    expect(screen.queryByRole("heading", { level: 1 })).not.toBeInTheDocument();
    expect(openButton()).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: i18n.t("setup.notesFirst.backToWelcome") }));

    expect(screen.queryByTestId("first-use-sample")).not.toBeInTheDocument();
    expect(sampleButton()).toHaveFocus();
    expect(onComplete).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
    expect(clipboardWrite).not.toHaveBeenCalled();
  });

  it("keeps ordinary notes reachable from the sample without choosing a destination", async () => {
    const onComplete = vi.fn();
    render(<NotesWelcome onComplete={onComplete} />);
    fireEvent.click(sampleButton());
    fireEvent.click(openButton());
    await waitFor(() => expect(openButton()).toBeEnabled());
    expect(onComplete).toHaveBeenCalledExactlyOnceWith();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("keeps Open notes behind the citation dialog boundary and restores citation focus", () => {
    const onComplete = vi.fn();
    render(<NotesWelcome onComplete={onComplete} />);
    fireEvent.click(sampleButton());
    fireEvent.click(screen.getByRole("button", { name: i18n.t("firstUse.sample.skipToResult") }));
    const citation = screen.getByTestId("sample-citation-sample-cite-1");
    citation.focus();
    fireEvent.click(citation);

    expect(screen.getByTestId("first-use-sample")).toHaveAttribute("inert");
    expect(screen.getByTestId("first-use-sample")).toContainElement(openButton());
    expect(screen.getByRole("dialog")).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(citation).toHaveFocus();
    expect(onComplete).not.toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("routes Bring my data to sources without importing sample content", async () => {
    const onComplete = vi.fn();
    render(<NotesWelcome onComplete={onComplete} />);
    enterSampleAi();
    fireEvent.click(screen.getByRole("button", { name: i18n.t("firstUse.sample.bringCta") }));
    await waitFor(() => expect(openButton()).toBeEnabled());
    expect(onComplete).toHaveBeenCalledExactlyOnceWith("sources");
    expect(invoke).not.toHaveBeenCalled();
    expect(clipboardWrite).not.toHaveBeenCalled();
  });

  it.each([
    ["ChatGPT", "connections"],
    ["Codex", "connect-agent"],
    ["Claude", "connect-agent"],
  ])("routes %s to %s", async (client, destination) => {
    const onComplete = vi.fn();
    render(<NotesWelcome onComplete={onComplete} />);
    enterSampleAi();
    fireEvent.click(screen.getByRole("tab", { name: client }));
    fireEvent.click(screen.getByRole("button", { name: i18n.t("firstUse.sample.connectCta") }));
    await waitFor(() => expect(openButton()).toBeEnabled());
    expect(onComplete).toHaveBeenCalledExactlyOnceWith(destination);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("keeps the selected sample screen after failure and allows a retry", async () => {
    const first = deferred();
    const retry = deferred();
    const onComplete = vi.fn()
      .mockImplementationOnce(() => first.promise)
      .mockImplementationOnce(() => retry.promise);
    render(<NotesWelcome onComplete={onComplete} />);
    enterSampleAi();
    fireEvent.click(screen.getByRole("tab", { name: "Claude" }));
    const connect = screen.getByRole("button", { name: i18n.t("firstUse.sample.connectCta") });
    fireEvent.click(connect);
    await act(async () => first.reject(new Error("could not save completion")));

    expect(screen.getByRole("alert")).toHaveTextContent(i18n.t("setup.notesFirst.openFailed"));
    expect(screen.getByRole("tab", { name: "Claude" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("/handoff")).toBeInTheDocument();
    expect(screen.getByTestId("first-use-sample").parentElement).not.toHaveAttribute("inert");
    fireEvent.click(connect);
    expect(onComplete).toHaveBeenNthCalledWith(2, "connect-agent");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await act(async () => retry.resolve());
    expect(openButton()).toBeEnabled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("guards duplicate and competing sample actions while completion is pending", async () => {
    const completion = deferred();
    const onComplete = vi.fn(() => completion.promise);
    render(<NotesWelcome onComplete={onComplete} />);
    enterSampleAi();
    const connect = screen.getByRole("button", { name: i18n.t("firstUse.sample.connectCta") });
    const bringData = screen.getByRole("button", { name: i18n.t("firstUse.sample.bringCta") });
    const back = screen.getByRole("button", { name: i18n.t("setup.notesFirst.backToWelcome") });
    act(() => {
      connect.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      connect.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      bringData.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      back.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      openButton().dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(onComplete).toHaveBeenCalledExactlyOnceWith("connections");
    expect(openButton()).toBeDisabled();
    expect(screen.getByTestId("first-use-sample").parentElement).toHaveAttribute("inert");
    fireEvent.click(screen.getByRole("tab", { name: "Codex" }));
    expect(screen.getByRole("tab", { name: "ChatGPT" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByTestId("first-use-sample")).toBeInTheDocument();
    await act(async () => completion.resolve());
    expect(screen.getByTestId("first-use-sample").parentElement).not.toHaveAttribute("inert");
    expect(invoke).not.toHaveBeenCalled();
  });
});
