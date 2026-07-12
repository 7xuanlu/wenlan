// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "../../i18n";

const mocks = vi.hoisted(() => ({
  getSetupStatus: vi.fn(),
  getExternalLlm: vi.fn(),
  getDaemonVersion: vi.fn(),
}));
vi.mock("../../lib/tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/tauri")>();
  return { ...actual, ...mocks };
});

import ActiveIntelligenceStrip from "./ActiveIntelligenceStrip";

const BASE_STATUS = {
  setup_completed: true,
  mode: "basic-memory",
  anthropic_key_configured: false,
  local_model_selected: null,
  local_model_loaded: null,
  local_model_cached: false,
};

function renderStrip() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ActiveIntelligenceStrip />
    </QueryClientProvider>
  );
}

describe("ActiveIntelligenceStrip", () => {
  beforeEach(() => {
    Object.values(mocks).forEach((m) => m.mockReset());
    mocks.getDaemonVersion.mockResolvedValue("0.12.0");
    mocks.getSetupStatus.mockResolvedValue(BASE_STATUS);
    mocks.getExternalLlm.mockResolvedValue([null, null]);
  });

  it("anthropic key configured tops the chain", async () => {
    mocks.getSetupStatus.mockResolvedValue({ ...BASE_STATUS, anthropic_key_configured: true });
    renderStrip();
    expect(await screen.findByText("Serving: Anthropic")).toBeInTheDocument();
  });

  it("0.12 external config shows configured (unverified), never serving", async () => {
    mocks.getExternalLlm.mockResolvedValue(["http://localhost:11434/v1", "llama3.2"]);
    renderStrip();
    expect(await screen.findByText(/configured \(unverified\)/)).toBeInTheDocument();
    expect(screen.queryByText("Serving: external endpoint")).not.toBeInTheDocument();
  });

  it("0.13 external loaded shows serving", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    mocks.getSetupStatus.mockResolvedValue({
      ...BASE_STATUS,
      external_llm: { configured: true, loaded: true },
    });
    mocks.getExternalLlm.mockResolvedValue(["https://api.openai.com/v1", "gpt-4o-mini"]);
    renderStrip();
    expect(await screen.findByText("Serving: external endpoint")).toBeInTheDocument();
  });

  it("0.13 configured-but-not-loaded shows restart pending", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    mocks.getSetupStatus.mockResolvedValue({
      ...BASE_STATUS,
      external_llm: { configured: true, loaded: false },
    });
    mocks.getExternalLlm.mockResolvedValue(["https://api.openai.com/v1", "gpt-4o-mini"]);
    renderStrip();
    expect(await screen.findByText(/restart pending/)).toBeInTheDocument();
  });

  it("on-device model loaded shows serving on-device", async () => {
    mocks.getSetupStatus.mockResolvedValue({ ...BASE_STATUS, local_model_loaded: "qwen3-4b" });
    renderStrip();
    expect(await screen.findByText("Serving: on-device model")).toBeInTheDocument();
  });

  it("nothing configured shows basic memory", async () => {
    renderStrip();
    expect(await screen.findByText(/Basic memory/)).toBeInTheDocument();
  });
});
