// SPDX-License-Identifier: AGPL-3.0-only
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../../i18n";
import {
  listAgents,
  listOnboardingMilestones,
  listRegisteredSources,
  type AgentConnection,
  type MilestoneRecord,
  type Page,
  type RegisteredSource,
} from "../../../lib/tauri";
import {
  ESTABLISHED_LIBRARY_PAGES,
  GettingStarted,
  gettingStartedProgress,
} from "../GettingStarted";

vi.mock("../../../lib/tauri", () => ({
  listAgents: vi.fn(),
  listOnboardingMilestones: vi.fn(),
  listRegisteredSources: vi.fn(),
}));

function page(overrides: Partial<Page> = {}): Page {
  return {
    id: "page-1",
    title: "Trip plan",
    summary: null,
    content: "",
    entity_id: null,
    domain: null,
    space: null,
    source_memory_ids: [],
    version: 1,
    status: "active",
    created_at: "2026-10-01T00:00:00Z",
    last_compiled: "2026-10-01T00:00:00Z",
    last_modified: "2026-10-01T00:00:00Z",
    creation_kind: "authored",
    ...overrides,
  } as Page;
}

function agent(name: string, overrides: Partial<AgentConnection> = {}): AgentConnection {
  return {
    id: `agent-${name}`,
    name,
    display_name: null,
    agent_type: "",
    description: null,
    enabled: true,
    trust_level: "full",
    last_seen_at: 1_790_000_000,
    memory_count: 0,
    created_at: 1_790_000_000,
    updated_at: 1_790_000_000,
    ...overrides,
  };
}

const folder = { id: "src-1", source_type: "directory", path: "/notes" } as RegisteredSource;
const recall = { id: "first-recall", first_triggered_at: 1, acknowledged_at: null, payload: null } as MilestoneRecord;

function renderCard(pages: Page[] = [], handlers: Partial<Record<"onWriteNote" | "onConnectTool" | "onAddSources" | "onSeeExample", () => void>> = {}) {
  const props = {
    onWriteNote: vi.fn(),
    onConnectTool: vi.fn(),
    onAddSources: vi.fn(),
    onSeeExample: vi.fn(),
    ...handlers,
  };
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <GettingStarted pages={pages} pagesReady {...props} />
    </QueryClientProvider>,
  );
  return props;
}

const title = () => i18n.t("firstUse.checklist.title");

describe("gettingStartedProgress", () => {
  it("reads every step from the library as it stands", () => {
    expect(gettingStartedProgress({ pages: [], agents: [], sources: [], milestones: [] }))
      .toEqual({ note: false, tool: false, sources: false, recall: false });
    expect(gettingStartedProgress({
      pages: [page()],
      agents: [agent("claude-code")],
      sources: [folder],
      milestones: [recall],
    })).toEqual({ note: true, tool: true, sources: true, recall: true });
  });

  it("does not count Wenlan's own identities or a tool that never called", () => {
    const progress = gettingStartedProgress({
      pages: [],
      agents: [
        agent("wenlan-app"),
        agent("wenlan-setup"),
        agent("wenlan-cli"),
        agent("manual"),
        agent("codex", { last_seen_at: null, memory_count: 0 }),
      ],
      sources: [],
      milestones: [],
    });
    expect(progress.tool).toBe(false);
  });

  it("counts imported source pages as brought-in material and distilled pages as AI use", () => {
    expect(gettingStartedProgress({
      pages: [page({ creation_kind: "imported" })], agents: [], sources: [], milestones: [],
    }).sources).toBe(true);
    expect(gettingStartedProgress({
      pages: [page({ creation_kind: "distilled" })], agents: [], sources: [], milestones: [],
    }).recall).toBe(true);
  });
});

describe("GettingStarted", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    vi.mocked(listAgents).mockResolvedValue([]);
    vi.mocked(listRegisteredSources).mockResolvedValue([]);
    vi.mocked(listOnboardingMilestones).mockResolvedValue([]);
  });

  it("offers each unfinished step with an action that leads to it", async () => {
    vi.mocked(listAgents).mockResolvedValue([agent("claude-code")]);
    const props = renderCard();

    expect(await screen.findByRole("heading", { name: title() })).toBeInTheDocument();
    expect(screen.getByText(i18n.t("firstUse.checklist.progress", { done: 1, total: 4 }))).toBeInTheDocument();
    expect(screen.getByTestId("getting-started-tool")).toHaveAttribute("data-done", "true");
    expect(screen.queryByRole("button", { name: i18n.t("firstUse.checklist.steps.tool.action") })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: i18n.t("firstUse.checklist.steps.note.action") }));
    fireEvent.click(screen.getByRole("button", { name: i18n.t("firstUse.checklist.steps.sources.action") }));
    fireEvent.click(screen.getByRole("button", { name: i18n.t("firstUse.checklist.steps.recall.action") }));
    expect(props.onWriteNote).toHaveBeenCalledOnce();
    expect(props.onAddSources).toHaveBeenCalledOnce();
    expect(props.onSeeExample).toHaveBeenCalledOnce();
    expect(props.onConnectTool).not.toHaveBeenCalled();
  });

  it("stays away when a read fails rather than claim a step is undone", async () => {
    vi.mocked(listAgents).mockRejectedValue(new Error("daemon down"));
    renderCard();
    await vi.waitFor(() => expect(listAgents).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByTestId("getting-started")).toBeNull();
  });

  it("does not appear for an established library or once every step is done", async () => {
    const many = Array.from({ length: ESTABLISHED_LIBRARY_PAGES }, (_, i) => page({ id: `p${i}` }));
    renderCard(many);
    await vi.waitFor(() => expect(listOnboardingMilestones).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByTestId("getting-started")).toBeNull();
  });

  it("hides when every step is done", async () => {
    vi.mocked(listAgents).mockResolvedValue([agent("codex")]);
    vi.mocked(listRegisteredSources).mockResolvedValue([folder]);
    vi.mocked(listOnboardingMilestones).mockResolvedValue([recall]);
    renderCard([page()]);
    await vi.waitFor(() => expect(listOnboardingMilestones).toHaveBeenCalled());
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByTestId("getting-started")).toBeNull();
  });

  it("can be hidden, remembers that, and can be shown again", async () => {
    renderCard();
    fireEvent.click(await screen.findByRole("button", { name: i18n.t("firstUse.checklist.hide") }));
    expect(screen.queryByTestId("getting-started")).toBeNull();
    expect(localStorage.getItem("onboarding:gettingStartedDismissed")).toBe("1");

    fireEvent.click(screen.getByRole("button", { name: i18n.t("firstUse.checklist.reopen") }));
    expect(screen.getByRole("heading", { name: title() })).toBeInTheDocument();
    expect(localStorage.getItem("onboarding:gettingStartedDismissed")).toBeNull();
  });
});
