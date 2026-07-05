// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../i18n";
import Sidebar from "./Sidebar";

const listAgentsMock = vi.hoisted(() => vi.fn().mockResolvedValue([]));

vi.mock("../../lib/tauri", () => ({
  getMemoryStats: vi.fn().mockResolvedValue({ total: 0, new_today: 0, confirmed: 0, domains: [] }),
  listAgents: listAgentsMock,
}));

vi.mock("./IdentityCard", () => ({ default: () => <div data-testid="identity-card" /> }));
vi.mock("./SpaceList", () => ({ default: () => <div data-testid="space-list">Spaces</div> }));
vi.mock("./EntitySuggestions", () => ({ default: () => <div data-testid="entity-suggestions" /> }));

function renderSidebar(extraProps: Record<string, unknown> = {}) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <Sidebar
        collapsed={false}
        onSelectSpace={() => {}}
        onEntityClick={() => {}}
        onNavigateHome={() => {}}
        onNavigateLog={() => {}}
        onNavigateGraph={() => {}}
        {...(extraProps as any)}
      />
    </QueryClientProvider>,
  );
}

describe("Sidebar", () => {
  beforeEach(async () => {
    listAgentsMock.mockResolvedValue([]);
    await i18n.changeLanguage("en");
  });

  it("places Home in the primary nav and routes it through onNavigateHome", async () => {
    const user = userEvent.setup();
    const onNavigateHome = vi.fn();
    renderSidebar({ onNavigateHome });

    const home = screen.getByRole("button", { name: "Home" });
    const spaces = screen.getByTestId("space-list");

    expect(home.compareDocumentPosition(spaces) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    await user.click(home);

    expect(onNavigateHome).toHaveBeenCalledTimes(1);
  });

  it("labels the memory view as Memories and keeps Decisions out of the left nav", () => {
    renderSidebar();

    expect(screen.getByRole("button", { name: "Memories" })).toBeInTheDocument();
    expect(screen.queryByText("Memory Log")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Decisions" })).not.toBeInTheDocument();
  });

  it("places Memories and Graph above Spaces", () => {
    renderSidebar();

    const memories = screen.getByRole("button", { name: "Memories" });
    const graph = screen.getByRole("button", { name: "Graph" });
    const spaces = screen.getByTestId("space-list");

    expect(memories.compareDocumentPosition(spaces) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(graph.compareDocumentPosition(spaces) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("places a Sources tab above Spaces and routes it through onNavigateSources", async () => {
    const user = userEvent.setup();
    const onNavigateSources = vi.fn();
    renderSidebar({ onNavigateSources });

    const sources = screen.getByRole("button", { name: "Sources" });
    const spaces = screen.getByTestId("space-list");

    // Sources sits in the primary nav, above the Spaces list.
    expect(sources.compareDocumentPosition(spaces) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    await user.click(sources);
    expect(onNavigateSources).toHaveBeenCalledTimes(1);
  });

  it("omits the Sources tab when source navigation is not wired", () => {
    renderSidebar();

    expect(screen.queryByRole("button", { name: "Sources" })).not.toBeInTheDocument();
  });

  it("uses the account card as the only sidebar footer item", () => {
    renderSidebar();

    const spaces = screen.getByTestId("space-list");
    const account = screen.getByTestId("identity-card");

    expect(spaces.compareDocumentPosition(account) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Wenlan" })).not.toBeInTheDocument();
  });

  it("keeps the account card as the footer after locale changes", async () => {
    await i18n.changeLanguage("zh-Hant");
    renderSidebar();

    expect(screen.getByTestId("identity-card")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Wenlan 文瀾" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Wenlan" })).not.toBeInTheDocument();
  });

  it("does not show agent connection status in the footer", async () => {
    listAgentsMock.mockResolvedValue([
      { id: "agent-1", name: "Codex", enabled: true },
      { id: "agent-2", name: "Claude", enabled: true },
    ]);

    renderSidebar();

    expect(await screen.findByTestId("identity-card")).toBeInTheDocument();
    expect(screen.queryByText(/agents? connected/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/No agents connected/i)).not.toBeInTheDocument();
  });
});
