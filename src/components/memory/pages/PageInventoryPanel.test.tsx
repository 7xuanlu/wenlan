// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../../i18n";
import type { Page } from "../../../lib/tauri";
import { PageInventoryPanel } from "./PageInventoryPanel";

const { listAllActivePagesMock, listAllDraftPagesMock, explicitActiveMock, explicitDraftMock } = vi.hoisted(() => ({
  listAllActivePagesMock: vi.fn().mockResolvedValue([]),
  listAllDraftPagesMock: vi.fn().mockResolvedValue([]),
  explicitActiveMock: vi.fn(), explicitDraftMock: vi.fn(),
}));
vi.mock("./listAllPages", () => ({
  EXPLICIT_BROWSE_QUERY_POLICY: { refetchOnWindowFocus: false, refetchOnReconnect: false },
  listAllActivePages: listAllActivePagesMock, listAllDraftPages: listAllDraftPagesMock,
  listAllActivePagesExplicitBrowse: explicitActiveMock, listAllDraftPagesExplicitBrowse: explicitDraftMock,
}));

function page(id: string, title: string, overrides: Partial<Page> = {}): Page {
  return {
    id, title, status: "active", summary: null, content: "", entity_id: null, domain: null,
    source_memory_ids: [], version: 1, created_at: "2026-07-16T00:00:00Z",
    last_compiled: "2026-07-16T00:00:00Z", last_modified: "2026-07-16T00:00:00Z", ...overrides,
  };
}
function renderPanel(props: Partial<React.ComponentProps<typeof PageInventoryPanel>> = {}) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return { queryClient, ...render(
    <QueryClientProvider client={queryClient}>
      <PageInventoryPanel onBrowse={() => {}} onOpenPage={() => {}} onOpenDraft={() => {}} {...props} />
    </QueryClientProvider>,
  ) };
}

describe("PageInventoryPanel", () => {
  beforeEach(async () => {
    listAllActivePagesMock.mockReset().mockResolvedValue([]);
    listAllDraftPagesMock.mockReset().mockResolvedValue([]);
    explicitActiveMock.mockReset().mockResolvedValue([]); explicitDraftMock.mockReset().mockResolvedValue([]);
    await i18n.changeLanguage("en");
  });

  it("browses collections using the shared explicit browse queries without duplicate note links or entity shadows", async () => {
    explicitActiveMock.mockResolvedValue([
      page("file", "Project", { storage_path: "historic-project.md", space: "Work" }),
      page("loose", "Other"), page("entity", "Person shadow", { creation_kind: "entity" }),
    ]);
    explicitDraftMock.mockResolvedValue([page("draft", "Loose thought", { status: "draft" })]);
    const onBrowse = vi.fn();
    const { queryClient } = renderPanel({ browsing: true, inventoryScope: "files", onBrowse });
    expect(await screen.findByRole("button", { name: "Note files" })).toHaveAttribute("aria-current", "page");
    expect(screen.queryByRole("button", { name: /^Open / })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Expand / })).not.toBeInTheDocument();
    expect(screen.queryByText("Work")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "All notes" })).toHaveTextContent("3");
    await userEvent.setup().click(screen.getByRole("button", { name: "Drafts" }));
    expect(onBrowse).toHaveBeenCalledWith("drafts");
    expect(queryClient.getQueryData(["pages", "active"])).toBeDefined();
    expect(listAllActivePagesMock).not.toHaveBeenCalled(); expect(listAllDraftPagesMock).not.toHaveBeenCalled();
  });

  it("opens the selected physical group and presents actual filenames with title tooltips", async () => {
    const note = page("a", "A title with spaces", { storage_path: "real-name.md" });
    listAllActivePagesMock.mockResolvedValue([note, page("other", "Legacy note")]);
    const onOpenPage = vi.fn(); const onBrowse = vi.fn();
    renderPanel({ currentPageId: "a", onOpenPage, onBrowse });
    const button = await screen.findByRole("button", { name: "Open A title with spaces" });
    expect(explicitActiveMock).not.toHaveBeenCalled();
    expect(explicitDraftMock).not.toHaveBeenCalled();
    expect(button).toHaveTextContent("real-name.md");
    expect(button).toHaveAttribute("title", "A title with spaces");
    expect(button).toHaveAttribute("aria-current", "page");
    const user = userEvent.setup();
    await user.click(button); expect(onOpenPage).toHaveBeenCalledWith(note);
    await user.click(screen.getByRole("button", { name: "Collapse Note files" }));
    expect(screen.queryByRole("button", { name: "Open A title with spaces" })).not.toBeInTheDocument();
    expect(onBrowse).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Expand Note files" }));
    await user.click(screen.getByRole("button", { name: "Note files" }));
    expect(onBrowse).toHaveBeenCalledWith("files");
    expect(screen.queryByRole("button", { name: "Open Legacy note" })).not.toBeInTheDocument();
  });

  it("preserves draft routing and Space context while using title display", async () => {
    listAllDraftPagesMock.mockResolvedValue([page("draft", "", { status: "draft", storage_path: "draft.md", space: "Work" })]);
    const onOpenDraft = vi.fn();
    renderPanel({ currentPageId: "draft", onOpenDraft });
    const draft = await screen.findByRole("button", { name: "Open Untitled draft" });
    expect(draft).toHaveTextContent("Untitled draft");
    expect(screen.queryByRole("button", { name: "Note files" })).not.toBeInTheDocument();
    await userEvent.setup().click(draft);
    expect(onOpenDraft).toHaveBeenCalledWith("draft", "Work");
  });

  it("supports separate keyboard disclosure and collection navigation", async () => {
    listAllActivePagesMock.mockResolvedValue([page("file", "Keyboard note", { storage_path: "keys.md" })]);
    const onBrowse = vi.fn();
    renderPanel({ onBrowse });
    const disclosure = await screen.findByRole("button", { name: "Expand Note files" });
    disclosure.focus();
    const user = userEvent.setup();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("button", { name: "Collapse Note files" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "Open Keyboard note" })).toBeInTheDocument();
    expect(onBrowse).not.toHaveBeenCalled();
    await user.tab();
    expect(screen.getByRole("button", { name: "Note files" })).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(onBrowse).toHaveBeenCalledWith("files");
  });

  it("exposes title and path matches during browsing without changing explicit browse queries", async () => {
    explicitActiveMock.mockResolvedValue([
      page("a", "Budget", { storage_path: "project-ledger.md" }), page("b", "Project plan"), page("c", "Other"),
    ]);
    renderPanel({ browsing: true });
    await screen.findByRole("button", { name: "Note files" });
    const user = userEvent.setup();
    const filter = screen.getByRole("searchbox", { name: "Filter notes" });
    await user.type(filter, "project");
    expect(screen.getByRole("button", { name: "Open Budget" })).toHaveTextContent("project-ledger.md");
    expect(screen.getByRole("button", { name: "Open Project plan" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open Other" })).not.toBeInTheDocument();
    await user.clear(filter);
    expect(screen.queryByRole("button", { name: /^Open / })).not.toBeInTheDocument();
    expect(listAllActivePagesMock).not.toHaveBeenCalled(); expect(listAllDraftPagesMock).not.toHaveBeenCalled();
  });

  it("shows loading until both status queries finish", async () => {
    let resolveDrafts!: (pages: Page[]) => void;
    explicitDraftMock.mockReturnValue(new Promise<Page[]>((resolve) => { resolveDrafts = resolve; }));
    explicitActiveMock.mockResolvedValue([page("a", "Available page")]);
    renderPanel({ browsing: true });
    expect(screen.getByRole("status")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "All notes" })).not.toBeInTheDocument();
    resolveDrafts([]);
    await screen.findByRole("button", { name: "All notes" });
  });

  it("does not present a partial inventory when one status fails and retries both queries", async () => {
    listAllActivePagesMock.mockResolvedValue([page("a", "Available page")]);
    listAllDraftPagesMock.mockRejectedValueOnce(new Error("draft list unavailable")).mockResolvedValue([]);
    renderPanel({ currentPageId: "a" });
    expect(await screen.findByRole("alert")).toHaveTextContent("Pages couldn't be loaded.");
    expect(screen.queryByRole("button", { name: "All notes" })).not.toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Open Available page" })).toBeInTheDocument());
    expect(listAllActivePagesMock).toHaveBeenCalledTimes(2); expect(listAllDraftPagesMock).toHaveBeenCalledTimes(2);
  });

  it("keeps collection controls available in an empty inventory and provides local no-match feedback", async () => {
    renderPanel({ browsing: true });
    await screen.findByRole("button", { name: "All notes" });
    expect(screen.getByRole("button", { name: "Drafts" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Other notes" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Note files" })).not.toBeInTheDocument();
    await userEvent.setup().type(screen.getByRole("searchbox", { name: "Filter notes" }), "absent");
    expect(screen.getByText(i18n.t("pages.overview.noMatches"))).toBeInTheDocument();
  });
});
