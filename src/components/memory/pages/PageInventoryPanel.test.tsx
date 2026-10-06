// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../../i18n";
import type { Page } from "../../../lib/tauri";
import { PageInventoryPanel } from "./PageInventoryPanel";

const {
  listAllActivePagesMock,
  listAllDraftPagesMock,
  explicitActiveMock,
  explicitDraftMock,
} = vi.hoisted(() => ({
  listAllActivePagesMock: vi.fn().mockResolvedValue([]),
  listAllDraftPagesMock: vi.fn().mockResolvedValue([]),
  explicitActiveMock: vi.fn(),
  explicitDraftMock: vi.fn(),
}));

vi.mock("./listAllPages", () => ({
  listAllActivePages: listAllActivePagesMock,
  listAllDraftPages: listAllDraftPagesMock,
  listAllActivePagesExplicitBrowse: explicitActiveMock,
  listAllDraftPagesExplicitBrowse: explicitDraftMock,
}));

function page(id: string, title: string, status = "active"): Page {
  return {
    id, title, status,
    summary: null, content: "", entity_id: null, domain: null,
    source_memory_ids: [], version: 1,
    created_at: "2026-07-16T00:00:00Z",
    last_compiled: "2026-07-16T00:00:00Z",
    last_modified: "2026-07-16T00:00:00Z",
  };
}

function renderPanel(props: Partial<React.ComponentProps<typeof PageInventoryPanel>> = {}) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return {
    queryClient,
    ...render(
      <QueryClientProvider client={queryClient}>
        <PageInventoryPanel onOpenPage={() => {}} onOpenDraft={() => {}} {...props} />
      </QueryClientProvider>,
    ),
  };
}

describe("PageInventoryPanel", () => {
  beforeEach(async () => {
    listAllActivePagesMock.mockReset().mockResolvedValue([]);
    listAllDraftPagesMock.mockReset().mockResolvedValue([]);
    explicitActiveMock.mockReset();
    explicitDraftMock.mockReset();
    await i18n.changeLanguage("en");
  });

  it("lists every active page and draft using passive queries, excluding entity shadows", async () => {
    const pages = Array.from({ length: 15 }, (_, index) => page(String(index), "Note " + index));
    const entity = page("entity", "Person shadow");
    entity.creation_kind = "entity";
    const draft = page("draft", "Loose thought", "draft");
    listAllActivePagesMock.mockResolvedValue([...pages, entity]);
    listAllDraftPagesMock.mockResolvedValue([draft]);
    const { queryClient } = renderPanel();

    expect(await screen.findByRole("button", { name: "Open Note 14" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^Open Note / })).toHaveLength(15);
    expect(screen.getByRole("button", { name: "Open Loose thought" })).toHaveTextContent("Draft");
    expect(screen.queryByRole("button", { name: "Open Person shadow" })).not.toBeInTheDocument();
    expect(queryClient.getQueryData(["pages", "inventory", "passive", "active"])).toBeDefined();
    expect(explicitActiveMock).not.toHaveBeenCalled();
    expect(explicitDraftMock).not.toHaveBeenCalled();
  });

  it("filters locally without changing the global header search", async () => {
    listAllActivePagesMock.mockResolvedValue([page("a", "Budget"), page("b", "Project plan")]);
    const user = userEvent.setup();
    renderPanel();

    const filter = screen.getByRole("searchbox", { name: "Filter notes" });
    await screen.findByRole("button", { name: "Open Project plan" });
    await user.type(filter, "project");
    expect(screen.getByRole("button", { name: "Open Project plan" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open Budget" })).not.toBeInTheDocument();
    expect(explicitActiveMock).not.toHaveBeenCalled();
  });

  it("does not present a partial inventory as complete when one status fails", async () => {
    listAllActivePagesMock.mockResolvedValue([page("a", "Available page")]);
    listAllDraftPagesMock.mockRejectedValueOnce(new Error("draft list unavailable")).mockResolvedValue([]);
    const user = userEvent.setup();
    renderPanel();

    expect(await screen.findByRole("alert")).toHaveTextContent("Pages couldn't be loaded.");
    expect(screen.queryByRole("button", { name: "Open Available page" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Open Available page" })).toBeInTheDocument());
  });
});
