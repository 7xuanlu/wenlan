// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import "../../i18n";

const mocks = vi.hoisted(() => ({
  getApiKey: vi.fn(),
  setApiKey: vi.fn(),
  getModelChoice: vi.fn(),
  setModelChoice: vi.fn(),
}));
vi.mock("../../lib/tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/tauri")>();
  return { ...actual, ...mocks };
});

import { ApiKeyCard } from "./IntelligenceSetup";

function renderCard(qc: QueryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })) {
  render(
    <QueryClientProvider client={qc}>
      <ApiKeyCard />
    </QueryClientProvider>
  );
  return qc;
}

describe("ApiKeyCard", () => {
  // NOTE: reset runs in afterEach, not beforeEach — see
  // src/hooks/useDaemonVersion.test.tsx for why. Resetting a vi.fn() in
  // beforeEach immediately before it's reconfigured with a resolved value
  // and consumed by a react-query queryFn deterministically triggers a
  // false-positive "unhandled rejection" failure on this project's pinned
  // vitest 4.1.5 + @tanstack/react-query 5.100.9 + React 19 combination.
  afterEach(() => {
    Object.values(mocks).forEach((m) => m.mockReset());
  });

  beforeEach(() => {
    mocks.getApiKey.mockResolvedValue(null);
    mocks.setApiKey.mockResolvedValue(undefined);
    mocks.getModelChoice.mockResolvedValue([null, null]);
    mocks.setModelChoice.mockResolvedValue(undefined);
  });

  it("invalidates setup-status, external-llm, and external-llm-key-configured after saving a key (strip staleness fix)", async () => {
    const qc = renderCard();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");

    await userEvent.type(screen.getByPlaceholderText("sk-ant-api03-..."), "sk-ant-test-key");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(mocks.setApiKey).toHaveBeenCalledWith("sk-ant-test-key"));
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["apiKey"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["setup-status"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["external-llm"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["external-llm-key-configured"] });
  });

  it("invalidates the same keys after clearing a configured key", async () => {
    mocks.getApiKey.mockResolvedValue("sk-ant-***configured");
    const qc = renderCard();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");

    await screen.findByText("sk-ant-***configured");
    await userEvent.click(screen.getByRole("button", { name: "Clear" }));

    await waitFor(() => expect(mocks.setApiKey).toHaveBeenCalledWith(""));
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["apiKey"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["setup-status"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["external-llm"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["external-llm-key-configured"] });
  });

  it("offers a Get-a-key link to the Anthropic console when no key is set", async () => {
    // getApiKey → null (set in beforeEach) renders the password-input
    // branch (not the masked state).
    vi.mocked(shellOpen).mockClear();
    renderCard();
    await userEvent.click(await screen.findByRole("button", { name: /Get a key/ }));
    expect(shellOpen).toHaveBeenCalledWith(
      "https://console.anthropic.com/settings/keys",
    );
  });
});
