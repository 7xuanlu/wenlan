// SPDX-License-Identifier: AGPL-3.0-only
import { afterEach, describe, expect, it, vi } from "vitest";
import { MemoryAutosave, type MemoryAutosaveRecord } from "./memoryAutosave";

const record = (content = "Original source", id = "memory-a"): MemoryAutosaveRecord => ({ source_id: id, content });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => { resolve = yes; });
  return { promise, resolve };
}

function setup() {
  let canonical = record();
  const read = vi.fn(async (id: string) => id === canonical.source_id ? canonical : null);
  const write = vi.fn(async (id: string, content: string, _expectedContent: string) => { canonical = record(content, id); });
  const onChange = vi.fn();
  const onCanonical = vi.fn();
  const autosave = new MemoryAutosave({ read, write, onChange, onCanonical });
  autosave.reset(canonical);
  return { autosave, read, write, onChange, onCanonical, setCanonical: (value: MemoryAutosaveRecord | null) => { canonical = value ?? record("", "deleted"); } };
}

afterEach(() => vi.useRealTimers());

describe("MemoryAutosave", () => {
  it("debounces a changed source for 600ms and preserves its exact text", async () => {
    vi.useFakeTimers();
    const { autosave, write } = setup();
    autosave.setSource("  changed\r\nsource  ");
    await vi.advanceTimersByTimeAsync(599);
    expect(write).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(write).toHaveBeenCalledWith("memory-a", "  changed\r\nsource  ", "Original source");
    expect(autosave.snapshot).toMatchObject({ baseline: { content: "  changed\r\nsource  " }, dirty: false, phase: "idle" });
    autosave.dispose();
  });

  it("serially saves newer typing after confirming the first snapshot", async () => {
    const { autosave, write, read } = setup();
    const first = deferred<void>();
    write.mockReturnValueOnce(first.promise);
    read.mockResolvedValueOnce(record()).mockResolvedValueOnce(record("First snapshot"))
      .mockResolvedValueOnce(record("First snapshot")).mockResolvedValueOnce(record("Newer local text"));
    autosave.setSource("First snapshot");
    const flushing = autosave.flush();
    await Promise.resolve();
    expect(write).toHaveBeenCalledTimes(1);
    autosave.setSource("Newer local text");
    first.resolve();
    expect(await flushing).toBe(true);
    expect(write.mock.calls).toEqual([
      ["memory-a", "First snapshot", "Original source"], ["memory-a", "Newer local text", "First snapshot"],
    ]);
    expect(autosave.snapshot).toMatchObject({ baseline: { content: "Newer local text" }, dirty: false });
    autosave.dispose();
  });

  it("does not save during IME composition and flushes after composition ends", async () => {
    vi.useFakeTimers();
    const { autosave, write } = setup();
    autosave.setComposing(true);
    autosave.setSource("中文輸入");
    await vi.advanceTimersByTimeAsync(1000);
    expect(await autosave.flush()).toBe(false);
    expect(write).not.toHaveBeenCalled();
    autosave.setComposing(false);
    await vi.advanceTimersByTimeAsync(600);
    expect(write).toHaveBeenCalledOnce();
    autosave.dispose();
  });

  it("re-reads before writing and stops on a changed or deleted canonical record", async () => {
    const { autosave, write, read, setCanonical } = setup();
    setCanonical(record("Remote edit"));
    autosave.setSource("Local draft");
    expect(await autosave.flush()).toBe(false);
    expect(read).toHaveBeenCalledWith("memory-a");
    expect(write).not.toHaveBeenCalled();
    expect(autosave.snapshot).toMatchObject({ phase: "conflict", latest: { content: "Remote edit" }, dirty: true });
    autosave.reset(record());
    setCanonical(null);
    autosave.setSource("Draft after deletion");
    expect(await autosave.flush()).toBe(false);
    expect(autosave.snapshot).toMatchObject({ phase: "conflict", latest: null, dirty: true });
    expect(write).not.toHaveBeenCalled();
    autosave.dispose();
  });

  it("clears a clean session after external deletion and permits a clean flush", async () => {
    const { autosave, write } = setup();
    autosave.observeCanonical(null);
    expect(autosave.snapshot).toMatchObject({ baseline: null, source: "", phase: "idle", dirty: false });
    expect(await autosave.flush()).toBe(true);
    expect(write).not.toHaveBeenCalled();
    autosave.dispose();
  });

  it("preserves a dirty draft in conflict when the canonical record is deleted", async () => {
    const { autosave, write } = setup();
    autosave.setSource("Draft to preserve");
    autosave.observeCanonical(null);
    expect(autosave.snapshot).toMatchObject({ phase: "conflict", latest: null, source: "Draft to preserve", dirty: true });
    expect(await autosave.flush()).toBe(false);
    expect(write).not.toHaveBeenCalled();
    autosave.dispose();
  });

  it("reconciles an ambiguous write only on explicit retry when readback has the submitted text", async () => {
    const { autosave, write, read, onCanonical } = setup();
    write.mockRejectedValueOnce(new Error("response lost"));
    read.mockResolvedValueOnce(record()).mockResolvedValueOnce(record("Possibly saved"));
    autosave.setSource("Possibly saved");
    expect(await autosave.flush()).toBe(false);
    expect(autosave.snapshot).toMatchObject({ phase: "error", dirty: true });
    expect(write).toHaveBeenCalledOnce();
    expect(await autosave.retry()).toBe(true);
    expect(write).toHaveBeenCalledOnce();
    expect(onCanonical).toHaveBeenCalledWith(record("Possibly saved"));
    expect(autosave.snapshot).toMatchObject({ baseline: { content: "Possibly saved" }, dirty: false, phase: "idle" });
    autosave.dispose();
  });

  it("reconciles a successful write with a failed readback on explicit retry without writing twice", async () => {
    const { autosave, write, read } = setup();
    read.mockResolvedValueOnce(record()).mockRejectedValueOnce(new Error("readback unavailable"));
    autosave.setSource("Committed but unconfirmed");
    expect(await autosave.flush()).toBe(false);
    expect(autosave.snapshot).toMatchObject({ phase: "error", dirty: true });
    expect(await autosave.retry()).toBe(true);
    expect(write).toHaveBeenCalledOnce();
    expect(autosave.snapshot).toMatchObject({ baseline: { content: "Committed but unconfirmed" }, dirty: false });
    autosave.dispose();
  });

  it("conflicts when the post-write readback contains a different canonical value", async () => {
    const { autosave, read, write } = setup();
    read.mockResolvedValueOnce(record()).mockResolvedValueOnce(record("Changed during save"));
    autosave.setSource("Local submitted text");
    expect(await autosave.flush()).toBe(false);
    expect(write).toHaveBeenCalledOnce();
    expect(autosave.snapshot).toMatchObject({ phase: "conflict", latest: { content: "Changed during save" }, dirty: true });
    autosave.dispose();
  });

  it("explicitly retries the same pending value if the canonical source is still at baseline", async () => {
    const { autosave, write, read } = setup();
    write.mockRejectedValueOnce(new Error("offline"));
    read.mockResolvedValueOnce(record()).mockResolvedValueOnce(record()).mockResolvedValueOnce(record());
    autosave.setSource("Retry this");
    expect(await autosave.flush()).toBe(false);
    expect(await autosave.retry()).toBe(true);
    expect(read).toHaveBeenCalledTimes(4); // failed preflight, retry check, replay preflight, and readback
    expect(write.mock.calls).toEqual([
      ["memory-a", "Retry this", "Original source"],
      ["memory-a", "Retry this", "Original source"],
    ]);
    autosave.dispose();
  });

  it("keeps a differing external refresh as a conflict while a dirty draft is open", () => {
    const { autosave, write } = setup();
    autosave.setSource("Local draft");
    autosave.observeCanonical(record("Remote edit"));
    expect(autosave.snapshot).toMatchObject({ phase: "conflict", latest: { content: "Remote edit" }, dirty: true });
    expect(write).not.toHaveBeenCalled();
    autosave.dispose();
  });

  it("adopts a clean external refresh", () => {
    const { autosave, onChange } = setup();
    onChange.mockClear();
    autosave.observeCanonical(record("Remote refresh"));
    expect(autosave.snapshot).toMatchObject({ baseline: { content: "Remote refresh" }, source: "Remote refresh", dirty: false, phase: "idle" });
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ source: "Remote refresh", dirty: false }));
    autosave.dispose();
  });

  it("does not start a preflighted write if IME composition begins while reading", async () => {
    vi.useFakeTimers();
    const { autosave, read, write } = setup();
    const preflight = deferred<MemoryAutosaveRecord | null>();
    read.mockReturnValueOnce(preflight.promise);
    autosave.setSource("Before composition");
    const flushing = autosave.flush();
    autosave.setComposing(true);
    preflight.resolve(record());
    expect(await flushing).toBe(false);
    expect(write).not.toHaveBeenCalled();
    expect(autosave.snapshot).toMatchObject({ phase: "idle", source: "Before composition", dirty: true });
    autosave.setSource("Final composition");
    autosave.setComposing(false);
    await vi.advanceTimersByTimeAsync(600);
    expect(write).toHaveBeenCalledWith("memory-a", "Final composition", "Original source");
    autosave.dispose();
  });

  it("resumes an explicit retry after composition starts during its canonical read", async () => {
    vi.useFakeTimers();
    const { autosave, read, write } = setup();
    write.mockRejectedValueOnce(new Error("offline"));
    autosave.setSource("Retry after composition");
    expect(await autosave.flush()).toBe(false);
    const canonical = deferred<MemoryAutosaveRecord | null>();
    read.mockReturnValueOnce(canonical.promise);
    const retrying = autosave.retry();
    autosave.setComposing(true);
    canonical.resolve(record());
    expect(await retrying).toBe(false);
    expect(autosave.snapshot).toMatchObject({ phase: "idle", dirty: true, source: "Retry after composition" });
    expect(write).toHaveBeenCalledOnce();
    autosave.setComposing(false);
    await vi.advanceTimersByTimeAsync(600);
    expect(write).toHaveBeenCalledTimes(2);
    expect(autosave.snapshot).toMatchObject({ baseline: { content: "Retry after composition" }, dirty: false });
    autosave.dispose();
  });

  it("ignores own pending write observations and reconciles through the post-write read", async () => {
    const { autosave, write, read } = setup();
    const gate = deferred<void>();
    write.mockReturnValueOnce(gate.promise);
    read.mockResolvedValueOnce(record()).mockResolvedValueOnce(record("Submitted"));
    autosave.setSource("Submitted");
    const flushing = autosave.flush();
    await Promise.resolve();
    expect(write).toHaveBeenCalledOnce();
    autosave.observeCanonical(record("Submitted"));
    expect(autosave.snapshot.phase).toBe("saving");
    gate.resolve();
    expect(await flushing).toBe(true);
    expect(autosave.snapshot).toMatchObject({ baseline: { content: "Submitted" }, dirty: false, phase: "idle" });
    autosave.dispose();
  });

  it("keeps whitespace-only input local as empty validation and never deletes", async () => {
    vi.useFakeTimers();
    const { autosave, write } = setup();
    autosave.setSource(" \r\n\t ");
    expect(await autosave.flush()).toBe(false);
    await vi.advanceTimersByTimeAsync(3000);
    expect(write).not.toHaveBeenCalled();
    expect(autosave.snapshot).toMatchObject({ validation: "empty", dirty: true, phase: "idle" });
    autosave.dispose();
  });

  it("flushes immediately and returns the in-flight drain to concurrent callers", async () => {
    const { autosave, write, setCanonical } = setup();
    const gate = deferred<void>();
    write.mockReturnValueOnce(gate.promise);
    autosave.setSource("Immediate");
    const first = autosave.flush();
    const second = autosave.flush();
    await Promise.resolve();
    expect(write).toHaveBeenCalledOnce();
    setCanonical(record("Immediate"));
    gate.resolve();
    expect(await Promise.all([first, second])).toEqual([true, true]);
    autosave.dispose();
  });

  it("does not apply a late result or callback after switching IDs or disposing", async () => {
    const { autosave, write, onCanonical } = setup();
    const gate = deferred<void>();
    write.mockReturnValueOnce(gate.promise);
    autosave.setSource("Old session edit");
    const oldFlush = autosave.flush();
    autosave.reset(record("New session", "memory-b"));
    gate.resolve();
    expect(await oldFlush).toBe(false);
    expect(autosave.snapshot).toMatchObject({ baseline: { id: "memory-b", content: "New session" }, dirty: false });
    expect(onCanonical).not.toHaveBeenCalled();

    const second = deferred<void>();
    write.mockReturnValueOnce(second.promise);
    autosave.setSource("Disposed session edit");
    const disposedFlush = autosave.flush();
    autosave.dispose();
    second.resolve();
    expect(await disposedFlush).toBe(false);
    expect(onCanonical).not.toHaveBeenCalled();
  });
});
