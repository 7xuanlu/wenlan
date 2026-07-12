// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "../../i18n";
import { resources } from "../../i18n/resources";

const mocks = vi.hoisted(() => ({
  detectMcpClients: vi.fn(),
  writeMcpConfig: vi.fn(),
  getWenlanMcpEntry: vi.fn(),
  clipboardWrite: vi.fn(),
}));
vi.mock("../../lib/tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/tauri")>();
  return { ...actual, ...mocks };
});

import ClientSetupList from "./ClientSetupList";

const CLIENTS = [
  { name: "Claude Code", client_type: "claude_code", config_path: "~/.claude.json", detected: true, already_configured: false },
  { name: "Codex CLI", client_type: "codex_cli", config_path: "~/.codex/config.toml", detected: true, already_configured: false },
  { name: "Cursor", client_type: "cursor", config_path: "~/.cursor/mcp.json", detected: true, already_configured: false },
  { name: "Claude Desktop", client_type: "claude_desktop", config_path: "~/Library/.../config.json", detected: true, already_configured: false },
  { name: "Gemini CLI", client_type: "gemini_cli", config_path: "~/.gemini/settings.json", detected: true, already_configured: false },
];

function renderList(qc: QueryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })) {
  return render(
    <QueryClientProvider client={qc}>
      <ClientSetupList />
    </QueryClientProvider>,
  );
}

/** Finds the row shell (ClientSetupList's `rounded-lg` wrapper div) for a
 *  given client name, so assertions can be scoped `within` a single card
 *  instead of matching anywhere in the document. */
function rowFor(name: string) {
  return screen.getByText(name).closest("div.rounded-lg") as HTMLElement;
}

describe("ClientSetupList — §9.3 plugin-first matrix", () => {
  afterEach(() => Object.values(mocks).forEach((m) => m.mockReset()));
  beforeEach(() => {
    mocks.detectMcpClients.mockResolvedValue(CLIENTS);
    mocks.getWenlanMcpEntry.mockResolvedValue({ command: "npx", args: ["-y", "wenlan-mcp"] });
    mocks.writeMcpConfig.mockResolvedValue(undefined);
    mocks.clipboardWrite.mockResolvedValue(undefined);
  });

  it("Claude Code leads with the plugin commands", async () => {
    renderList();
    expect(await screen.findByText("claude plugin marketplace add 7xuanlu/wenlan")).toBeInTheDocument();
    expect(screen.getByText("claude plugin install wenlan@7xuanlu-wenlan")).toBeInTheDocument();
  });

  it("Codex leads with codex mcp add using the real command+args", async () => {
    renderList();
    expect(await screen.findByText("codex mcp add wenlan -- npx -y wenlan-mcp")).toBeInTheDocument();
  });

  it("Copy setup prompt writes the full agent prompt to the clipboard", async () => {
    renderList();
    const buttons = await screen.findAllByRole("button", { name: /Copy setup prompt/ });
    await userEvent.click(buttons[0]); // Claude Code card
    expect(mocks.clipboardWrite).toHaveBeenCalledTimes(1);
    expect(mocks.clipboardWrite.mock.calls[0][0]).toContain("claude plugin install wenlan@7xuanlu-wenlan");
  });

  it("GUI clients keep the one-click Set up as their primary action, CLI clients demote it under Advanced", async () => {
    renderList();
    // Cursor / Claude Desktop / Gemini CLI → exactly 3 primary "Set up" buttons.
    const setUps = await screen.findAllByRole("button", { name: "Set up" });
    expect(setUps).toHaveLength(3);

    // Claude Code / Codex CLI: config write demoted under an Advanced
    // <details>, and no primary "Set up" button survives on either row —
    // this is the exact regression a mutation deleting the demotion would
    // reintroduce (it re-adds a primary "Set up" to both CLI cards).
    await screen.findByText("claude plugin marketplace add 7xuanlu/wenlan");
    for (const name of ["Claude Code", "Codex CLI"]) {
      const row = rowFor(name);
      expect(within(row).getByText("Advanced")).toBeInTheDocument();
      expect(within(row).queryByRole("button", { name: "Set up" })).not.toBeInTheDocument();
    }
  });

  it("shipped copy never references .mcpb or .codex-plugin — DOM, the copied prompt, and every locale", async () => {
    const { container } = renderList();
    await screen.findByText("claude plugin install wenlan@7xuanlu-wenlan");
    expect(container.textContent).not.toContain(".mcpb");
    expect(container.textContent).not.toContain(".codex-plugin");

    // The setup prompt goes straight to the clipboard, never through the
    // DOM — assert on what was actually copied, not just what rendered.
    const buttons = await screen.findAllByRole("button", { name: /Copy setup prompt/ });
    await userEvent.click(buttons[0]);
    expect(mocks.clipboardWrite).toHaveBeenCalledTimes(1);
    const copiedPrompt = mocks.clipboardWrite.mock.calls[0][0] as string;
    expect(copiedPrompt).not.toContain(".mcpb");
    expect(copiedPrompt).not.toContain(".codex-plugin");

    // en is only one of three shipped locales — scan every connectMatrix
    // string in every locale, not just what happened to render/copy in en.
    for (const [locale, bundle] of Object.entries(resources)) {
      const connectMatrix = (bundle.translation as Record<string, unknown>).connectMatrix as Record<
        string,
        string
      >;
      for (const [key, value] of Object.entries(connectMatrix)) {
        expect(value, `${locale}.connectMatrix.${key}`).not.toContain(".mcpb");
        expect(value, `${locale}.connectMatrix.${key}`).not.toContain(".codex-plugin");
      }

      // These are shell commands, not prose — a locale/translation pass must
      // never "localize" a slug. Pin the exact byte value in every locale so
      // a drifted install string (e.g. zh-Hans slugging wenlan@7xuanlu-wenlan
      // down to wenlan@7xuanlu) fails loudly instead of shipping silently.
      expect(connectMatrix.claudeCodeCommand1, `${locale}.connectMatrix.claudeCodeCommand1`).toBe(
        "claude plugin marketplace add 7xuanlu/wenlan",
      );
      expect(connectMatrix.claudeCodeCommand2, `${locale}.connectMatrix.claudeCodeCommand2`).toBe(
        "claude plugin install wenlan@7xuanlu-wenlan",
      );
      expect(connectMatrix.codexCommand, `${locale}.connectMatrix.codexCommand`).toBe(
        "codex mcp add wenlan -- {{cmd}}",
      );

      // The copy-pasteable setup-prompt strings embed these same commands
      // verbatim (inside backticks) — pin them there too, so a translation
      // pass can't drift the copy-pasted command while leaving the
      // standalone key alone.
      expect(
        connectMatrix.claudeCodePrompt,
        `${locale}.connectMatrix.claudeCodePrompt`,
      ).toContain("claude plugin marketplace add 7xuanlu/wenlan");
      expect(
        connectMatrix.claudeCodePrompt,
        `${locale}.connectMatrix.claudeCodePrompt`,
      ).toContain("claude plugin install wenlan@7xuanlu-wenlan");
      expect(connectMatrix.codexPrompt, `${locale}.connectMatrix.codexPrompt`).toContain(
        "codex mcp add wenlan -- {{cmd}}",
      );
    }
  });

  it("undetected CLI clients show Not detected, not install commands", async () => {
    mocks.detectMcpClients.mockResolvedValue([
      { name: "Claude Code", client_type: "claude_code", config_path: "~/.claude.json", detected: false, already_configured: false },
      { name: "Codex CLI", client_type: "codex_cli", config_path: "~/.codex/config.toml", detected: false, already_configured: false },
    ]);
    renderList();
    for (const name of ["Claude Code", "Codex CLI"]) {
      const nameEl = await screen.findByText(name);
      const row = nameEl.closest("div.rounded-lg") as HTMLElement;
      expect(within(row).getByText("Not installed")).toBeInTheDocument();
      expect(within(row).queryByText("Advanced")).not.toBeInTheDocument();
      expect(within(row).queryByText(/claude plugin marketplace add/)).not.toBeInTheDocument();
      expect(within(row).queryByRole("button", { name: /Copy setup prompt/ })).not.toBeInTheDocument();
    }
  });

  it("Codex Copy setup prompt is disabled — never copies a broken command — while the MCP entry is unresolved", async () => {
    mocks.getWenlanMcpEntry.mockImplementation(() => new Promise(() => {})); // never resolves
    renderList();
    const nameEl = await screen.findByText("Codex CLI");
    const row = nameEl.closest("div.rounded-lg") as HTMLElement;
    const copyButton = within(row).getByRole("button", { name: /Copy setup prompt/ });
    expect(copyButton).toBeDisabled();
    expect(within(row).queryByText(/^codex mcp add wenlan --\s*$/)).not.toBeInTheDocument();
    await userEvent.click(copyButton);
    expect(mocks.clipboardWrite).not.toHaveBeenCalled();
  });

  it("Codex Copy setup prompt is disabled when the MCP entry query fails", async () => {
    mocks.getWenlanMcpEntry.mockRejectedValue(new Error("ipc failed"));
    renderList();
    const nameEl = await screen.findByText("Codex CLI");
    const row = nameEl.closest("div.rounded-lg") as HTMLElement;
    const copyButton = within(row).getByRole("button", { name: /Copy setup prompt/ });
    expect(copyButton).toBeDisabled();
    await userEvent.click(copyButton);
    expect(mocks.clipboardWrite).not.toHaveBeenCalled();
  });
});
