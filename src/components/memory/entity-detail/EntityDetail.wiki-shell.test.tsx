// SPDX-License-Identifier: AGPL-3.0-only
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { useGraphPalette } from "../../../lib/graph/palette";
import AtlasTypeFilters from "../AtlasTypeFilters";
import AtlasTooltip from "../AtlasTooltip";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../../i18n";
import type { EntityDetail as EntityDetailRecord } from "../../../lib/tauri";
import EntityDetail from "../EntityDetail";

vi.mock("../../../lib/tauri", () => ({
  getEntityDetail: vi.fn(),
  updateObservation: vi.fn().mockResolvedValue(undefined),
  deleteObservation: vi.fn().mockResolvedValue(undefined),
  addObservation: vi.fn().mockResolvedValue(undefined),
  confirmObservation: vi.fn().mockResolvedValue(undefined),
  confirmEntity: vi.fn().mockResolvedValue(undefined),
  deleteEntity: vi.fn().mockResolvedValue(undefined),
  search: vi.fn().mockResolvedValue([]),
  FACET_COLORS: {},
}));

vi.mock("../FocusGraph", () => ({ default: () => <div /> }));
vi.mock("../AtlasView", () => ({ default: NestedAtlasControls }));

function NestedAtlasControls() {
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const palette = useGraphPalette();
  return (
    <>
      <AtlasTypeFilters
        types={[["person", 1]]} excluded={excluded} palette={palette}
        onToggle={() => setExcluded((current) => current.size ? new Set() : new Set(["person"]))}
        onReset={() => setExcluded(new Set())}
      />
      <AtlasTooltip content="Nested Atlas help"><button type="button">Atlas help</button></AtlasTooltip>
    </>
  );
}

import { getEntityDetail } from "../../../lib/tauri";

const detail: EntityDetailRecord = {
  entity: {
    id: "entity-ada",
    name: "Ada Lovelace",
    entity_type: "person",
    domain: "computing",
    space: "History of Computing",
    source_agent: "research-agent",
    confidence: 0.87,
    confirmed: true,
    created_at: 1_700_000_000,
    updated_at: 1_700_086_400,
    memory_count: 1,
    status: "established",
    established_by: "manual",
  },
  observations: [
    {
      id: "obs-1",
      entity_id: "entity-ada",
      content: "Wrote the first published algorithm",
      source_agent: "research-agent",
      confidence: 0.8,
      confirmed: true,
      created_at: 1_700_000_100,
    },
  ],
  relations: [
    {
      id: "relation-1",
      relation_type: "collaborated with",
      direction: "outgoing",
      entity_id: "entity-babbage",
      entity_name: "Charles Babbage",
      entity_type: "person",
      source_agent: "research-agent",
      created_at: 1_700_000_200,
    },
  ],
};

function renderEntity(record: EntityDetailRecord = detail) {
  vi.mocked(getEntityDetail).mockResolvedValue(record);
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, refetchOnWindowFocus: false },
      mutations: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <EntityDetail
        entityId={record.entity.id}
        onBack={vi.fn()}
        onEntityClick={vi.fn()}
        onMemoryClick={vi.fn()}
      />
    </QueryClientProvider>,
  );
}

describe("EntityDetail wiki shell", () => {
  afterEach(() => vi.unstubAllGlobals());
  afterEach(() => vi.restoreAllMocks());
  beforeEach(async () => {
    // jsdom has no layout. Represent the visible controls in these focus tests;
    // real hidden/visible geometry is covered by the browser regressions.
    vi.spyOn(HTMLElement.prototype, "getClientRects").mockImplementation(
      () => [new DOMRect(0, 0, 100, 30)] as unknown as DOMRectList,
    );
    vi.clearAllMocks();
    await i18n.changeLanguage("en");
  });

  it("uses the shared document layout with a title-level overflow and no permanent context", async () => {
    const { container } = renderEntity();
    const title = await screen.findByRole("heading", { level: 1, name: "Ada Lovelace" });
    expect(container.firstElementChild).toHaveClass("page-detail", "entity-detail-dossier", "document-context-host");
    expect(container.querySelector(".page-detail-document")).toBeInTheDocument();
    expect(container.querySelector(".page-detail-prose")).toBeInTheDocument();
    expect(title.parentElement).toContainElement(screen.getByRole("button", { name: "Topic actions" }));
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(screen.queryByText("AL")).not.toBeInTheDocument();
    expect(container.querySelector(".page-detail-dateline")).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Connections" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Delete topic" })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "About" })).toBeInTheDocument();
  });

  it("opens metadata followed by Connections in a closeable context pane", async () => {
    renderEntity();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Info" }));
    const panel = screen.getByRole("dialog", { name: "Info" });
    const connections = within(panel).getByRole("heading", { name: "Connections" });
    const details = within(panel).getByRole("heading", { name: "Details" });
    expect(details.compareDocumentPosition(connections) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(panel).getByRole("combobox", { name: "Type" })).toHaveValue("person");
    expect(within(panel).getByRole("group", { name: "Connection map for Ada Lovelace" })).toBeInTheDocument();
    const ledger = within(panel).getByRole("group", { name: "Connections" });
    expect(within(ledger).getByRole("button", { name: /Charles Babbage/ })).toHaveAccessibleName("Charles Babbage (Person) · outgoing · collaborated with");
    await user.click(within(panel).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog", { name: "Info" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Info" })).toHaveFocus();
  });

  it("navigates the menu with Arrow, Home and End and returns focus on Escape", async () => {
    renderEntity();
    const user = userEvent.setup();
    const trigger = await screen.findByRole("button", { name: "Topic actions" });
    await user.click(trigger);
    const menu = screen.getByRole("menu", { name: "Topic actions" });
    expect(menu).toHaveAttribute("data-keyboard-navigation", "false");
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Archive", "Delete topic"]);
    await user.keyboard("{Escape}");
    expect(trigger).toHaveFocus();
    trigger.focus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menu")).toHaveAttribute("data-keyboard-navigation", "true");
    expect(screen.getByRole("menuitem", { name: "Archive" })).toHaveFocus();
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    await user.keyboard("{End}");
    expect(screen.getByRole("menuitem", { name: "Delete topic" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Archive" })).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(screen.getByRole("menuitem", { name: "Delete topic" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(screen.getByRole("menuitem", { name: "Archive" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(screen.getByRole("menuitem", { name: "Delete topic" })).toHaveFocus();
  });

  it("closing the wide pane preserves the central note editor and its draft", async () => {
    vi.stubGlobal("matchMedia", vi.fn((query: string) => ({
      matches: query === "(min-width: 1100px)", media: query, onchange: null,
      addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn(),
    })));
    renderEntity();
    const user = userEvent.setup();
    await screen.findByRole("heading", { level: 1, name: "Ada Lovelace" });
    const existingPanel = screen.queryByRole("complementary", { name: "Info" });
    if (!existingPanel) await user.click(await screen.findByRole("button", { name: "Info" }));
    const panel = screen.getByRole("complementary", { name: "Info" });
    await user.click(screen.getByRole("button", { name: "Edit information — Wrote the first published algorithm" }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " draft");
    // Closing the side pane preserves the unsaved row draft.
    fireEvent.click(within(panel).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("complementary", { name: "Info" })).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Edit information" })).toBe(input);
    expect(input).toHaveValue("Wrote the first published algorithm draft");

  });

  it.each([false, true])("keeps Atlas portals and nested keyboard dismissal inside the graph (wide=%s)", async (wide) => {
    vi.stubGlobal("matchMedia", vi.fn((query: string) => ({
      matches: wide && query === "(min-width: 1100px)", media: query, onchange: null,
      addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn(),
    })));
    renderEntity();
    const user = userEvent.setup();
    await screen.findByRole("heading", { level: 1, name: "Ada Lovelace" });
    if (!wide) await user.click(await screen.findByRole("button", { name: "Info" }));
    const context = screen.getByRole(wide ? "complementary" : "dialog", { name: "Info" });
    const expand = screen.getByRole("button", { name: "Full screen" });
    await user.click(expand);
    const graph = screen.getByRole("dialog", { name: "Full screen" });
    await user.click(within(graph).getByRole("button", { name: "Atlas" }));
    const filter = await within(graph).findByRole("button", { name: "Filter topic types" });
    await user.click(filter);
    const types = screen.getByRole("dialog", { name: "Topic types" });
    expect(graph).toContainElement(types);
    expect(types).toHaveFocus();
    await user.keyboard("{Tab}{Tab} ");
    expect(within(types).getByRole("button", { name: "Person" })).toHaveAttribute("aria-pressed", "false");
    expect(types).toContainElement(document.activeElement as HTMLElement);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Topic types" })).not.toBeInTheDocument();
    expect(graph).toBeInTheDocument();
    expect(context).toBeInTheDocument();
    expect(filter).toHaveFocus();
    await user.tab();
    expect(within(graph).getByRole("button", { name: "Atlas help" })).toHaveFocus();
    expect(graph).toContainElement(screen.getByRole("tooltip"));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    expect(graph).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Full screen" })).not.toBeInTheDocument();
    expect(context).toBeInTheDocument();
    expect(expand).toHaveFocus();
  });

  it("renders empty relationships and malformed numeric metadata without invalid text", async () => {
    renderEntity({
      entity: { ...detail.entity, confidence: Number.NaN, created_at: Number.MAX_VALUE, updated_at: Number.NEGATIVE_INFINITY },
      observations: [{ ...detail.observations[0], confidence: Number.POSITIVE_INFINITY }],
      relations: [],
    });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Info" }));
    expect(await screen.findByText("No connections recorded yet.")).toBeInTheDocument();
    expect(screen.queryAllByText(/NaN|Infinity|Invalid Date/)).toHaveLength(0);
  });
});
