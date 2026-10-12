// SPDX-License-Identifier: AGPL-3.0-only
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../../i18n";
import type { Observation } from "../../../lib/tauri";
import { EntityObservations } from "./EntityObservations";

vi.mock("../../../lib/tauri", () => ({
  addObservation: vi.fn(),
  deleteObservation: vi.fn(),
  updateObservation: vi.fn(),
}));
import { addObservation, deleteObservation, updateObservation } from "../../../lib/tauri";

const observation: Observation = {
  id: "obs-1", entity_id: "entity-1", content: "Information",
  source_agent: "human", confidence: 1, confirmed: false, created_at: 1_700_000_000,
};
const entryName = "Edit information — Information";

function renderObservations(
  observations: readonly Observation[] = [observation],
  onRegisterFlush?: (flush: (() => Promise<boolean>) | null) => void,
) {
  const queryClient = new QueryClient({ defaultOptions: {
    queries: { retry: false, gcTime: 0 }, mutations: { retry: false },
  } });
  const user = userEvent.setup();
  const rendered = render(
    <QueryClientProvider client={queryClient}>
      <EntityObservations
        entityId="entity-1"
        entityName="Example"
        observations={observations}
        onInvalidate={vi.fn()}
        onRegisterFlush={onRegisterFlush}
      />
    </QueryClientProvider>,
  );
  return { ...rendered, user, queryClient };
}

describe("EntityObservations direct deletion and autosave", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await i18n.changeLanguage("en");
    vi.mocked(addObservation).mockResolvedValue("obs-new");
    vi.mocked(deleteObservation).mockResolvedValue(undefined);
    vi.mocked(updateObservation).mockResolvedValue(undefined);
  });

  it("deletes an existing row with one direct action and no confirmation", async () => {
    const { user } = renderObservations();
    await user.click(await screen.findByRole("button", { name: "Delete information — Information" }));
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1"));
    expect(screen.queryByRole("button", { name: entryName })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add information" })).toBeInTheDocument();
    expect(updateObservation).not.toHaveBeenCalled();
  });

  it("keeps a cleared row in the editor until blur, then deletes without saving empty text", async () => {
    const { user } = renderObservations();
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.clear(input);
    expect(input).toHaveValue("");
    expect(updateObservation).not.toHaveBeenCalled();
    expect(deleteObservation).not.toHaveBeenCalled();
    fireEvent.blur(input);
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1"));
    expect(updateObservation).not.toHaveBeenCalled();
    expect(screen.queryByRole("textbox", { name: "Edit information" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add information" })).toBeInTheDocument();
  });

  it("leaves a blank editor open when an in-flight autosave finishes without a leave request", async () => {
    let resolveUpdate: (() => void) | undefined;
    vi.mocked(updateObservation).mockImplementationOnce(() => new Promise<void>((resolve) => { resolveUpdate = resolve; }));
    const flush: { current: (() => Promise<boolean>) | null } = { current: null };
    const { user } = renderObservations([observation], (value) => { flush.current = value; });
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " edited");
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("obs-1", "Information edited"));
    await user.clear(input);
    expect(input).toHaveValue("");
    resolveUpdate?.();
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Edit information" })).toHaveValue(""));
    expect(deleteObservation).not.toHaveBeenCalled();
    expect(await flush.current?.()).toBe(true);
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1"));
    expect(updateObservation).toHaveBeenCalledTimes(1);
    expect(updateObservation).not.toHaveBeenLastCalledWith("obs-1", "");
  });

  it("waits for a requested blur flush to delete after an in-flight update settles", async () => {
    let resolveUpdate: (() => void) | undefined;
    let resolveDelete: (() => void) | undefined;
    vi.mocked(updateObservation).mockImplementationOnce(() => new Promise<void>((resolve) => { resolveUpdate = resolve; }));
    vi.mocked(deleteObservation).mockImplementationOnce(() => new Promise<void>((resolve) => { resolveDelete = resolve; }));
    const flush: { current: (() => Promise<boolean>) | null } = { current: null };
    const { user } = renderObservations([observation], (value) => { flush.current = value; });
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " edited");
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("obs-1", "Information edited"));
    await user.clear(input);
    fireEvent.blur(input);
    let pendingFlush!: Promise<boolean>;
    act(() => { pendingFlush = flush.current!(); });
    resolveUpdate?.();
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1"));
    let settled = false;
    void pendingFlush.then(() => { settled = true; });
    await Promise.resolve();
    expect(settled).toBe(false);
    resolveDelete?.();
    expect(await pendingFlush).toBe(true);
    expect(updateObservation).toHaveBeenCalledTimes(1);
    expect(updateObservation).not.toHaveBeenLastCalledWith("obs-1", "");
  });

  it("leaves a cleared new draft open when its in-flight add finishes without a leave request", async () => {
    let resolveAdd: ((id: string) => void) | undefined;
    vi.mocked(addObservation).mockImplementationOnce(() => new Promise<string>((resolve) => { resolveAdd = resolve; }));
    const flush: { current: (() => Promise<boolean>) | null } = { current: null };
    const { user } = renderObservations([], (value) => { flush.current = value; });
    await user.click(await screen.findByRole("button", { name: "Add information" }));
    const input = screen.getByRole("textbox", { name: "Add information" });
    await user.type(input, "New information");
    await waitFor(() => expect(addObservation).toHaveBeenCalledExactlyOnceWith("entity-1", "New information", "human", 1));
    await user.clear(input);
    resolveAdd?.("obs-new");
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Add information" })).toHaveValue(""));
    expect(deleteObservation).not.toHaveBeenCalled();
    expect(await flush.current?.()).toBe(true);
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-new"));
    expect(updateObservation).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Add information" })).toBeInTheDocument();
  });

  it("deletes a newly added canonical row when blur is requested during its in-flight add", async () => {
    let resolveAdd: ((id: string) => void) | undefined;
    vi.mocked(addObservation).mockImplementationOnce(() => new Promise<string>((resolve) => { resolveAdd = resolve; }));
    const { user } = renderObservations([]);
    await user.click(await screen.findByRole("button", { name: "Add information" }));
    const input = screen.getByRole("textbox", { name: "Add information" });
    await user.type(input, "New information");
    await waitFor(() => expect(addObservation).toHaveBeenCalledExactlyOnceWith("entity-1", "New information", "human", 1));
    await user.clear(input);
    fireEvent.blur(input);
    resolveAdd?.("obs-new");
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-new"));
    expect(updateObservation).not.toHaveBeenCalled();
  });

  it("retains a failed delete and draft, blocks navigation flush, then retries deletion only", async () => {
    vi.mocked(deleteObservation).mockRejectedValueOnce(new Error("delete failed"));
    const flush: { current: (() => Promise<boolean>) | null } = { current: null };
    const { user } = renderObservations([observation], (value) => { flush.current = value; });
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " draft");
    await user.click(screen.getByRole("button", { name: "Delete information — Information" }));
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn’t delete. Your information is still here.");
    expect(screen.getByRole("textbox", { name: "Edit information" })).toHaveValue("Information draft");
    expect(updateObservation).not.toHaveBeenCalled();
    expect(await flush.current?.()).toBe(false);
    expect(deleteObservation).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: "Retry delete" }));
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledTimes(2));
    expect(updateObservation).not.toHaveBeenCalled();
    expect(screen.queryByRole("textbox", { name: "Edit information" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add information" })).toBeInTheDocument();
  });

  it("deletes the active row after its autosave has already failed", async () => {
    vi.mocked(updateObservation).mockRejectedValueOnce(new Error("offline"));
    const { user } = renderObservations();
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " draft");
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("obs-1", "Information draft"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't save. Try again.");

    await user.click(screen.getByRole("button", { name: "Delete information — Information" }));
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1"));
    expect(updateObservation).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("textbox", { name: "Edit information" })).not.toBeInTheDocument();
  });

  it("waits for an active in-flight autosave failure before deleting that same row", async () => {
    let rejectUpdate: ((error: Error) => void) | undefined;
    vi.mocked(updateObservation).mockImplementationOnce(() => new Promise<void>((_resolve, reject) => { rejectUpdate = reject; }));
    const { user } = renderObservations();
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " draft");
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("obs-1", "Information draft"));

    await user.click(screen.getByRole("button", { name: "Delete information — Information" }));
    expect(deleteObservation).not.toHaveBeenCalled();
    rejectUpdate?.(new Error("offline"));
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1"));
    expect(updateObservation).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("textbox", { name: "Edit information" })).not.toBeInTheDocument();
  });

  it("shows only delete recovery after both save and delete fail, then restores save recovery on Keep", async () => {
    vi.mocked(updateObservation).mockRejectedValueOnce(new Error("save offline"));
    vi.mocked(deleteObservation).mockRejectedValueOnce(new Error("delete offline"));
    const { user } = renderObservations();
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " draft");
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("obs-1", "Information draft"));
    await waitFor(() => expect(screen.getByRole("button", { name: "Retry save" })).toBeInTheDocument());

    await user.click(screen.getByRole("button", { name: "Delete information — Information" }));
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1"));
    await screen.findByText("Couldn’t delete. Your information is still here.");
    expect(screen.queryByRole("button", { name: "Retry save" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry delete" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep information" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Edit information" })).toHaveValue("Information draft");

    await user.click(screen.getByRole("button", { name: "Keep information" }));
    expect(screen.getByRole("textbox", { name: "Edit information" })).toHaveValue("Information draft");
    expect(screen.getByRole("button", { name: "Retry save" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry delete" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry save" }));
    await waitFor(() => expect(updateObservation).toHaveBeenCalledTimes(2));
    expect(updateObservation).toHaveBeenLastCalledWith("obs-1", "Information draft");
  });

  it("lets the user keep information after a failed clear-delete and continue editing", async () => {
    vi.mocked(deleteObservation).mockRejectedValueOnce(new Error("offline"));
    const flush: { current: (() => Promise<boolean>) | null } = { current: null };
    const { user } = renderObservations([observation], (value) => { flush.current = value; });
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.clear(input);
    fireEvent.blur(input);
    await waitFor(() => expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn’t delete. Your information is still here.");
    expect(await flush.current?.()).toBe(false);

    await user.click(screen.getByRole("button", { name: "Keep information" }));
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Edit information" })).toHaveValue("Information"));
    await user.type(screen.getByRole("textbox", { name: "Edit information" }), " revised");
    expect(await flush.current?.()).toBe(true);
    expect(updateObservation).toHaveBeenCalledExactlyOnceWith("obs-1", "Information revised");
    expect(deleteObservation).toHaveBeenCalledTimes(1);
  });

  it("keeps a nonblank unsaved draft when the user rejects a failed delete", async () => {
    vi.mocked(deleteObservation).mockRejectedValueOnce(new Error("offline"));
    const flush: { current: (() => Promise<boolean>) | null } = { current: null };
    const { user } = renderObservations([observation], (value) => { flush.current = value; });
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " draft");
    await user.click(screen.getByRole("button", { name: "Delete information — Information" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("textbox", { name: "Edit information" })).toHaveValue("Information draft");
    expect(updateObservation).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "Keep information" }));
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Edit information" })).toHaveValue("Information draft"));
    expect(await flush.current?.()).toBe(true);
    expect(updateObservation).toHaveBeenCalledExactlyOnceWith("obs-1", "Information draft");
    expect(deleteObservation).toHaveBeenCalledExactlyOnceWith("obs-1");
  });

  it("hides legacy blank rows and offers one unboxed add entry; blank new drafts are discarded", async () => {
    const blankRows = [
      { ...observation, id: "blank-1", content: "" },
      { ...observation, id: "blank-2", content: "" },
    ];
    const { user } = renderObservations(blankRows);
    expect(screen.getAllByRole("button", { name: "Add information" })).toHaveLength(1);
    expect(screen.queryByText("Add information about Example…")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Delete information/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add information" }));
    expect(screen.getByRole("textbox", { name: "Add information" })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(addObservation).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Add information" })).toBeInTheDocument();
  });

  it("keeps pause-to-autosave for nonblank edits", async () => {
    const { user } = renderObservations();
    await user.click(await screen.findByRole("button", { name: entryName }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " updated");
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("obs-1", "Information updated"), { timeout: 1500 });
    expect(input).toHaveValue("Information updated");
    expect(deleteObservation).not.toHaveBeenCalled();
  });
});
