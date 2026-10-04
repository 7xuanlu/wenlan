// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";

const emit = vi.hoisted(() => vi.fn());
vi.mock("./tauri-stubs", () => ({ emit }));

beforeEach(() => {
  vi.resetModules();
  emit.mockReset();
  window.history.replaceState({}, "", "/");
});

describe("remote reconnect fixture contract", () => {
  it("preserves the profile revision and grants with no off event", async () => {
    const { invokeRemoteFixture: invoke } = await import("./remote-access");
    const configured = await invoke("configure_remote_access", { space: "Review workspace" }) as { revision: string };
    await invoke("toggle_remote_access", { enabled: true, expectedRevision: configured.revision });
    const before = await invoke("get_remote_access_profile") as { revision: string };
    await invoke("approve_remote_pairing", { expectedRevision: before.revision });
    const grants = await invoke("list_remote_grants");
    emit.mockClear();
    await expect(invoke("reconnect_remote_access", { expectedRevision: before.revision })).resolves.toMatchObject({ status: "connected" });
    expect(await invoke("get_remote_access_profile")).toEqual(before);
    expect(await invoke("list_remote_grants")).toEqual(grants);
    expect(emit.mock.calls).toEqual([["remote-access-status", expect.objectContaining({ status: "connected" })]]);
    await expect(invoke("reconnect_remote_access", { expectedRevision: "stale" })).rejects.toThrow("Stale fixture revision");
    expect(await invoke("list_remote_grants")).toEqual(grants);
    await invoke("toggle_remote_access", { enabled: false });
    expect(await invoke("list_remote_grants")).toEqual({ items: [], cursor: null });
    const disabled = await invoke("get_remote_access_profile") as { revision: string };
    await expect(invoke("reconnect_remote_access", { expectedRevision: disabled.revision })).rejects.toThrow("Stale fixture revision");
  });
});
