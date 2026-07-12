import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { SetupWizard } from "./SetupWizard";
import { i18n } from "../i18n";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("../lib/tauri", () => ({
  importMemories: vi.fn(),
  shouldShowWizard: vi.fn().mockResolvedValue(true),
  setSetupCompleted: vi.fn().mockResolvedValue(undefined),
  detectMcpClients: vi.fn().mockResolvedValue([]),
  writeMcpConfig: vi.fn().mockResolvedValue(undefined),
  listAgents: vi.fn().mockResolvedValue([]),
  getWenlanMcpEntry: vi.fn().mockResolvedValue({ command: "npx", args: ["-y", "wenlan-mcp"] }),
  getRemoteAccessStatus: vi.fn().mockResolvedValue({ status: "off" }),
  toggleRemoteAccess: vi.fn().mockResolvedValue({ status: "off" }),
  rotateRemoteToken: vi.fn().mockResolvedValue("new-token"),
  testRemoteMcpConnection: vi.fn().mockResolvedValue({ ok: true, latency_ms: 42, error: null }),
  clipboardWrite: vi.fn().mockResolvedValue(undefined),
  getApiKey: vi.fn().mockResolvedValue(null),
  setApiKey: vi.fn().mockResolvedValue(undefined),
  getModelChoice: vi.fn().mockResolvedValue([null, null]),
  setModelChoice: vi.fn().mockResolvedValue(undefined),
  getOnDeviceModel: vi.fn().mockResolvedValue({
    loaded: null,
    selected: "qwen3-4b-instruct-2507",
    models: [{
      id: "qwen3-4b-instruct-2507",
      display_name: "Qwen3 4B",
      param_count: "4B",
      ram_required_gb: 8,
      file_size_gb: 2.7,
      cached: false,
    }],
  }),
  downloadOnDeviceModel: vi.fn().mockResolvedValue(undefined),
  getSystemInfo: vi.fn().mockResolvedValue({
    total_ram_gb: 16,
    available_ram_gb: 10,
    has_metal: true,
    has_cuda: false,
    os: "macOS",
    arch: "arm64",
    recommended_builtin: "qwen3-4b-instruct-2507",
  }),
  getDaemonVersion: vi.fn().mockResolvedValue("0.12.0"),
  daemonMeetsFloor: vi.fn().mockReturnValue(false),
  getExternalLlm: vi.fn().mockResolvedValue([null, null]),
  setExternalLlm: vi.fn().mockResolvedValue(undefined),
  testExternalLlm: vi.fn().mockResolvedValue({ response: "pong" }),
  listExternalModels: vi.fn().mockResolvedValue([]),
  getExternalLlmKeyConfigured: vi.fn().mockResolvedValue(false),
}));

import {
  detectMcpClients,
  writeMcpConfig,
  listAgents,
  setApiKey,
  clipboardWrite,
} from "../lib/tauri";

function renderWizard(
  props: {
    onComplete?: () => void;
    initialStep?: "welcome" | "intelligence-choice" | "import" | "connect" | "verify" | "done";
  } = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
    },
  });
  const onComplete = props.onComplete ?? vi.fn();
  return {
    onComplete,
    ...render(
      <QueryClientProvider client={queryClient}>
        <SetupWizard onComplete={onComplete} initialStep={props.initialStep} />
      </QueryClientProvider>,
    ),
  };
}

describe("SetupWizard", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await i18n.changeLanguage("en");
    (detectMcpClients as ReturnType<typeof vi.fn>).mockResolvedValue([]);
    (writeMcpConfig as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
    (listAgents as ReturnType<typeof vi.fn>).mockResolvedValue([]);
    (setApiKey as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
  });

  it("renders Welcome step by default", () => {
    renderWizard();
    expect(screen.getByText("Welcome to Wenlan")).toBeInTheDocument();
    expect(screen.getByText(/Where understanding compounds/i)).toBeInTheDocument();
    expect(screen.getByText("Everything stays on your device")).toBeInTheDocument();
  });

  it("renders Welcome step in Simplified Chinese when selected", async () => {
    await i18n.changeLanguage("zh-Hans");
    renderWizard();

    expect(screen.getByText("欢迎使用文澜")).toBeInTheDocument();
    expect(screen.getByText("所有内容都留在你的设备上")).toBeInTheDocument();
  });

  it('advances from Welcome to intelligence choice on "Get started" click', () => {
    renderWizard();
    fireEvent.click(screen.getByText("Get started"));
    expect(screen.getByText("Choose how Wenlan thinks")).toBeInTheDocument();
  });

  it("lets users save an API key from the intelligence step", async () => {
    renderWizard();
    fireEvent.click(screen.getByText("Get started"));
    fireEvent.click(screen.getByText("Cloud API"));

    fireEvent.change(screen.getByPlaceholderText("sk-ant-api03-..."), {
      target: { value: "sk-ant-test-key" },
    });
    fireEvent.click(screen.getByText("Save"));

    await waitFor(() => {
      expect(setApiKey).toHaveBeenCalledWith("sk-ant-test-key");
    });
  });

  it("intelligence step offers device, cloud, and local server", async () => {
    renderWizard();
    fireEvent.click(screen.getByText("Get started"));

    expect(screen.getByText("On this device")).toBeInTheDocument();
    expect(screen.getByText("Cloud API")).toBeInTheDocument();
    expect(screen.getByText("Local server")).toBeInTheDocument();
  });

  it("cloud pane lists Anthropic first and routes non-Anthropic vendors to the external card", async () => {
    renderWizard();
    fireEvent.click(screen.getByText("Get started"));
    fireEvent.click(screen.getByText("Cloud API"));

    // Anthropic pill renders the native key card by default.
    expect(screen.getByText("Anthropic")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("sk-ant-api03-...")).toBeInTheDocument();

    // pick OpenAI → AnyProviderCard appears (its title)
    await userEvent.click(screen.getByText("OpenAI"));
    expect(await screen.findByText("Any provider")).toBeInTheDocument();
  });

  it("local pane offers the external card scoped to local presets", async () => {
    renderWizard();
    fireEvent.click(screen.getByText("Get started"));
    fireEvent.click(screen.getByText("Local server"));

    expect(await screen.findByText("Any provider")).toBeInTheDocument();
  });

  it("import step offers chat history and vault side by side", async () => {
    renderWizard();
    fireEvent.click(screen.getByText("Get started"));
    fireEvent.click(screen.getByText("Continue"));

    expect(screen.getByText("Bring what you already know")).toBeInTheDocument();
    expect(screen.getByText("Chat history")).toBeInTheDocument();
    expect(screen.getByText("Import chat history")).toBeInTheDocument();
    // VaultConnectCard (Task 6), rendered inline as the second path, with its
    // own side-by-side section label (spec §2) above the card.
    expect(screen.getByText("Obsidian vault / notes folder")).toBeInTheDocument();
    expect(screen.getByText("Connect a notes folder")).toBeInTheDocument();
  });

  it("shows chat-history guidance after choosing the chat path and routes directly to connect", async () => {
    renderWizard();
    fireEvent.click(screen.getByText("Get started"));
    fireEvent.click(screen.getByText("Continue"));

    fireEvent.click(screen.getByText("Import chat history"));

    expect(screen.getByText("Import Memories")).toBeInTheDocument();
    expect(screen.getByText(/Settings > Sources/i)).toBeInTheDocument();

    fireEvent.click(screen.getByText("Skip"));

    await waitFor(() => {
      expect(screen.getByText("Choose tools to connect")).toBeInTheDocument();
    });
  });

  it("connect step separates detected and supported safe tools", async () => {
    (detectMcpClients as ReturnType<typeof vi.fn>).mockResolvedValue([
      {
        name: "Cursor",
        client_type: "cursor",
        config_path: "/path/to/cursor",
        detected: true,
        already_configured: false,
      },
      {
        name: "Claude Code",
        client_type: "claude_code",
        config_path: "/path/to/claude.json",
        detected: false,
        already_configured: false,
      },
    ]);

    renderWizard({ initialStep: "connect" });

    await waitFor(() => {
      expect(screen.getByText("Detected on your Mac")).toBeInTheDocument();
      expect(screen.getByText("Supported tools")).toBeInTheDocument();
    });

    expect(screen.getByText("Cursor")).toBeInTheDocument();
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    const [cursorCheckbox, claudeCheckbox] = screen.getAllByRole("checkbox");
    expect(cursorCheckbox).toBeEnabled();
    expect(claudeCheckbox).toBeDisabled();
  });

  it("undetected CLI clients render the disabled checkbox, not the plugin install path", async () => {
    (detectMcpClients as ReturnType<typeof vi.fn>).mockResolvedValue([
      {
        name: "Claude Code",
        client_type: "claude_code",
        config_path: "/path/to/claude.json",
        detected: false,
        already_configured: false,
      },
    ]);

    renderWizard({ initialStep: "connect" });

    await waitFor(() => {
      expect(screen.getByText("Claude Code")).toBeInTheDocument();
    });

    // An undetected CLI client can't run a plugin-install command against a
    // binary that isn't there — it must fall through to the generic
    // description, never the plugin path or its Copy button.
    expect(screen.queryByText(/claude plugin marketplace add/)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Copy setup prompt/ }),
    ).not.toBeInTheDocument();
  });

  it("connects selected detected tools on continue", async () => {
    (detectMcpClients as ReturnType<typeof vi.fn>).mockResolvedValue([
      {
        name: "Cursor",
        client_type: "cursor",
        config_path: "/path/to/cursor",
        detected: true,
        already_configured: false,
      },
    ]);

    renderWizard({ initialStep: "connect" });

    await waitFor(() => {
      expect(screen.getByText("Cursor")).toBeInTheDocument();
    });

    const [cursorCheckbox] = screen.getAllByRole("checkbox");
    await waitFor(() => {
      expect(cursorCheckbox).toBeChecked();
    });

    fireEvent.click(screen.getByText("Continue"));

    await waitFor(() => {
      expect(writeMcpConfig).toHaveBeenCalledWith("cursor");
      expect(screen.getByText("Waiting for your first agent...")).toBeInTheDocument();
    });
  });

  it("stays on connect step when MCP setup fails", async () => {
    (detectMcpClients as ReturnType<typeof vi.fn>).mockResolvedValue([
      {
        name: "Cursor",
        client_type: "cursor",
        config_path: "/path/to/cursor",
        detected: true,
        already_configured: false,
      },
    ]);
    (writeMcpConfig as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("setup failed"));

    renderWizard({ initialStep: "connect" });

    await waitFor(() => {
      expect(screen.getByText("Cursor")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Continue"));

    await waitFor(() => {
      expect(writeMcpConfig).toHaveBeenCalledWith("cursor");
      expect(screen.getByText("Choose tools to connect")).toBeInTheDocument();
      expect(screen.queryByText("Waiting for your first agent...")).not.toBeInTheDocument();
      expect(screen.getByText(/setup failed/i)).toBeInTheDocument();
    });
  });

  it("connect step leads detected CLI clients with the plugin path and unchecks their one-click default", async () => {
    (detectMcpClients as ReturnType<typeof vi.fn>).mockResolvedValue([
      {
        name: "Claude Code",
        client_type: "claude_code",
        config_path: "/path/to/claude.json",
        detected: true,
        already_configured: false,
      },
      {
        name: "Cursor",
        client_type: "cursor",
        config_path: "/path/to/cursor",
        detected: true,
        already_configured: false,
      },
    ]);

    renderWizard({ initialStep: "connect" });

    // Primary path: the plugin commands render inside the wizard row.
    expect(
      await screen.findByText("claude plugin marketplace add 7xuanlu/wenlan"),
    ).toBeInTheDocument();
    expect(screen.getByText("claude plugin install wenlan@7xuanlu-wenlan")).toBeInTheDocument();

    // One-click demoted for CLI clients: checkbox defaults OFF; GUI stays ON.
    const cursorCheckbox = screen.getByRole("checkbox", { name: "Cursor" });
    await waitFor(() => expect(cursorCheckbox).toBeChecked());
    expect(screen.getByRole("checkbox", { name: "Claude Code" })).not.toBeChecked();
    expect(screen.getByText("Or write the config for me")).toBeInTheDocument();
  });

  it("connect step Copy setup prompt copies the Codex prompt with the real command", async () => {
    (detectMcpClients as ReturnType<typeof vi.fn>).mockResolvedValue([
      {
        name: "Codex CLI",
        client_type: "codex_cli",
        config_path: "/path/to/config.toml",
        detected: true,
        already_configured: false,
      },
    ]);

    renderWizard({ initialStep: "connect" });

    expect(
      await screen.findByText("codex mcp add wenlan -- npx -y wenlan-mcp"),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Copy setup prompt" }));
    await waitFor(() => expect(clipboardWrite).toHaveBeenCalledTimes(1));
    expect(
      (clipboardWrite as ReturnType<typeof vi.fn>).mock.calls[0][0],
    ).toContain("codex mcp add wenlan -- npx -y wenlan-mcp");
  });

  it("verify step skips waiting UX when agents already wrote in the past", async () => {
    (listAgents as ReturnType<typeof vi.fn>).mockResolvedValue([
      {
        name: "Claude",
        client_type: "claude_code",
        first_seen_at: Math.floor(Date.now() / 1000) - 86400,
        last_seen_at: Math.floor(Date.now() / 1000) - 3600,
        memory_count: 5,
      },
    ]);

    renderWizard({ initialStep: "verify" });

    await waitFor(() => {
      expect(screen.getByText("You're all set.")).toBeInTheDocument();
    });

    expect(screen.getByText("Claude")).toBeInTheDocument();
    expect(screen.queryByText("Back")).not.toBeInTheDocument();
  });

  it("renders skip-path Done copy without a back button", async () => {
    renderWizard();

    fireEvent.click(screen.getByText("Get started"));
    fireEvent.click(screen.getByText("Continue"));
    fireEvent.click(screen.getByText("Skip"));

    await waitFor(() => {
      expect(screen.getByText("Choose tools to connect")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByText("Skip"));
    fireEvent.click(screen.getByText("Skip"));

    await waitFor(() => {
      expect(screen.getByText("Wenlan is ready.")).toBeInTheDocument();
    });

    expect(screen.queryByText("Back")).not.toBeInTheDocument();
  });

  it("Connect step renders RemoteAccessPanel (compact)", async () => {
    renderWizard({ initialStep: "connect" });
    await waitFor(() => {
      expect(screen.getByText(/Share with web-based AI tools/i)).toBeInTheDocument();
    });
  });

  it("Connect step remote toggle invokes toggleRemoteAccess", async () => {
    const { toggleRemoteAccess } = await import("../lib/tauri");
    renderWizard({ initialStep: "connect" });
    await screen.findByText(/Share with web-based AI tools/i);
    fireEvent.click(screen.getByRole("switch"));
    await waitFor(() => {
      expect(toggleRemoteAccess).toHaveBeenCalled();
    });
  });
});
