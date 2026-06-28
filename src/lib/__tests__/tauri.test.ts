import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("sources, page export, and knowledge wrappers", () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it("listRegisteredSources calls list_registered_sources", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { listRegisteredSources } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue([]);
    await listRegisteredSources();
    expect(invoke).toHaveBeenCalledWith("list_registered_sources");
  });

  it("addSource passes sourceType and path", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { addSource } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({
      id: "obsidian-vault",
      source_type: "obsidian",
      path: "/vault",
      status: "Active",
      last_sync: null,
      file_count: 0,
      memory_count: 0,
    });
    await addSource("obsidian", "/vault");
    expect(invoke).toHaveBeenCalledWith("add_source", {
      sourceType: "obsidian",
      path: "/vault",
    });
  });

  it("removeSource passes id", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { removeSource } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    await removeSource("obsidian-vault");
    expect(invoke).toHaveBeenCalledWith("remove_source", {
      id: "obsidian-vault",
    });
  });

  it("syncRegisteredSource passes id", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { syncRegisteredSource } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({
      files_found: 10,
      ingested: 5,
      skipped: 5,
      errors: 0,
    });
    await syncRegisteredSource("obsidian-vault");
    expect(invoke).toHaveBeenCalledWith("sync_registered_source", {
      id: "obsidian-vault",
    });
  });

  it("exportPageToObsidian passes pageId and vaultPath", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { exportPageToObsidian } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({
      path: "/path/to/file.md",
    });
    const result = await exportPageToObsidian("c123", "/vault");
    expect(result).toEqual({ path: "/path/to/file.md" });
    expect(invoke).toHaveBeenCalledWith("export_page_to_obsidian", {
      pageId: "c123",
      vaultPath: "/vault",
    });
  });

  it("testExternalLlm preserves the daemon response envelope", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { testExternalLlm } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue({
      response: "hello",
    });

    const result = await testExternalLlm("http://localhost:11434/v1", "qwen3");

    expect(result.response).toBe("hello");
    expect(invoke).toHaveBeenCalledWith("test_external_llm", {
      endpoint: "http://localhost:11434/v1",
      model: "qwen3",
    });
  });

  it("getKnowledgePath calls get_knowledge_path", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { getKnowledgePath } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(
      "/home/user/Origin/knowledge",
    );
    await getKnowledgePath();
    expect(invoke).toHaveBeenCalledWith("get_knowledge_path");
  });

  it("countKnowledgeFiles calls count_knowledge_files", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { countKnowledgeFiles } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(12);
    await countKnowledgeFiles();
    expect(invoke).toHaveBeenCalledWith("count_knowledge_files");
  });

  it("quitWenlanFull calls the Wenlan lifecycle command", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { quitWenlanFull } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    await quitWenlanFull();
    expect(invoke).toHaveBeenCalledWith("quit_wenlan_full");
  });

  it("quitOriginFull remains a legacy alias", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const { quitOriginFull } = await import("../tauri");
    (invoke as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    await quitOriginFull();
    expect(invoke).toHaveBeenCalledWith("quit_origin_full");
  });
});
