// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "../../../i18n";

const mocks = vi.hoisted(() => ({
  openDialog: vi.fn(),
  detectVault: vi.fn(),
  addSource: vi.fn(),
  syncRegisteredSource: vi.fn(),
  listRegisteredSources: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.openDialog }));
vi.mock("../../../lib/vaultDetection", () => ({ detectVault: mocks.detectVault }));
vi.mock("../../../lib/tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../../lib/tauri")>();
  return {
    ...actual,
    addSource: mocks.addSource,
    syncRegisteredSource: mocks.syncRegisteredSource,
    listRegisteredSources: mocks.listRegisteredSources,
  };
});

import VaultConnectCard from "./VaultConnectCard";

function renderCard(props: Partial<React.ComponentProps<typeof VaultConnectCard>> = {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <VaultConnectCard variant="wizard" {...props} />
    </QueryClientProvider>
  );
}

const SOURCE = {
  id: "s1",
  source_type: "obsidian",
  path: "/v",
  status: "Active",
  last_sync: null,
  file_count: 12,
  memory_count: 3,
};

describe("VaultConnectCard", () => {
  beforeEach(() => {
    Object.values(mocks).forEach((m) => m.mockReset());
    mocks.openDialog.mockResolvedValue("/v");
    mocks.addSource.mockResolvedValue(SOURCE);
    mocks.syncRegisteredSource.mockResolvedValue({
      files_found: 12, ingested: 12, skipped: 0, errors: 0,
    });
    mocks.listRegisteredSources.mockResolvedValue([SOURCE]);
  });

  it("zero-count detection warns but does NOT block submit (council change e)", async () => {
    mocks.detectVault.mockResolvedValue({
      isVault: false, sourceType: "directory", docCount: 0,
      countCapped: false, hasValidDoc: false, unreadable: false,
    });
    renderCard();
    await userEvent.click(screen.getByText("Browse…"));
    await waitFor(() =>
      expect(screen.getByText(/No notes found/)).toBeInTheDocument()
    );
    const connect = screen.getByRole("button", { name: "Connect" });
    expect(connect).toBeEnabled();
  });

  it("connects an obsidian vault: addSource + one-shot sync + indexed line", async () => {
    mocks.detectVault.mockResolvedValue({
      isVault: true, sourceType: "obsidian", docCount: 12,
      countCapped: false, hasValidDoc: true, unreadable: false,
    });
    renderCard();
    await userEvent.click(screen.getByText("Browse…"));
    await waitFor(() => expect(screen.getByText(/Detected \.obsidian\//)).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(mocks.addSource).toHaveBeenCalledWith("obsidian", "/v"));
    await waitFor(() => expect(mocks.syncRegisteredSource).toHaveBeenCalledWith("s1"));
    await waitFor(() =>
      expect(screen.getByText(/Indexed 12 files/)).toBeInTheDocument()
    );
  });

  it("surfaces daemon 4xx verbatim", async () => {
    mocks.detectVault.mockResolvedValue({
      isVault: false, sourceType: "directory", docCount: 3,
      countCapped: false, hasValidDoc: true, unreadable: false,
    });
    mocks.addSource.mockRejectedValue(new Error("path does not exist: /v"));
    renderCard();
    await userEvent.click(screen.getByText("Browse…"));
    await waitFor(() => expect(screen.getByText(/3 supported files/)).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    await waitFor(() =>
      expect(screen.getByText(/path does not exist: \/v/)).toBeInTheDocument()
    );
  });
});
