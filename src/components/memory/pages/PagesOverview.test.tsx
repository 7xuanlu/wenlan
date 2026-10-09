import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  distillReview,
  listPagesExplicitBrowse,
  listRefinements,
  type DistillReviewResponse,
  type Page,
} from "../../../lib/tauri";
import { PagesOverview } from "./PagesOverview";
import { DISTILL_REVIEW_SESSION_QUERY_KEY } from "./pageReviewSignals";
import type { WikiInventoryScope } from "./pageInventory";

vi.mock("../../../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../lib/tauri")>()),
  distillReview: vi.fn(),
  listPagesExplicitBrowse: vi.fn(),
  listRefinements: vi.fn(),
  listOnboardingMilestones: vi.fn().mockResolvedValue([]),
  getTruthStatus: vi.fn().mockResolvedValue(null),
}));

function page(overrides: Partial<Page> = {}): Page {
  return {
    id: "page-1",
    title: "Independent research note",
    summary: "A page can stand on its own.",
    content: "",
    entity_id: null,
    domain: null,
    space: null,
    source_memory_ids: [],
    version: 1,
    status: "active",
    created_at: "2026-07-01T00:00:00Z",
    last_compiled: "2026-07-01T00:00:00Z",
    last_modified: "2026-07-10T00:00:00Z",
    ...overrides,
  };
}

function renderOverview({
  onCreatePage = vi.fn(),
  onSelectDraft = vi.fn(),
  onSelectPage = vi.fn(),
  onSelectSpace = vi.fn(),
  inventoryScope = "all" as WikiInventoryScope,
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
} = {}) {
  const overview = (
    <QueryClientProvider client={queryClient}>
      <PagesOverview
        inventoryScope={inventoryScope}
        onCreatePage={onCreatePage}
        onSelectDraft={onSelectDraft}
        onSelectPage={onSelectPage}
        onSelectSpace={onSelectSpace}
      />
    </QueryClientProvider>
  );
  return {
    queryClient,
    onCreatePage,
    onSelectDraft,
    onSelectPage,
    onSelectSpace,
    ...render(overview),
  };
}

describe("PagesOverview reading workspace", () => {
  beforeEach(() => {
    vi.mocked(listPagesExplicitBrowse).mockReset();
    vi.mocked(listRefinements).mockReset();
    vi.mocked(distillReview).mockReset();
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([]);
  });

  it("shows localized reading guidance while creation lives beside the tabs", async () => {
    vi.mocked(listPagesExplicitBrowse).mockImplementation(async status => status === "draft"
      ? [page({ id: "draft", title: "Cached draft name", status: "draft" })]
      : [page({ id: "active", title: "Cached active name" })]);
    const discovery: DistillReviewResponse = {
      pages_created: 0,
      scoped: false,
      created_ids: [],
      pending: [{ source_ids: ["source"], contents: ["candidate content"], entity_name: "Passive candidate name", estimated_tokens: 20 }],
      stale_pages: [],
      stale_truncated: false,
      orphan_topics: [],
    };
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(DISTILL_REVIEW_SESSION_QUERY_KEY, discovery);
    const { container } = renderOverview({ queryClient });

    expect(await screen.findByRole("heading", { level: 1, name: "Open a note" })).toBeInTheDocument();
    expect(screen.getByText("Use Search to find a note, or select + to start writing.")).toBeInTheDocument();
    expect(screen.queryByTestId("wiki-create-options")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "New" })).not.toBeInTheDocument();
    expect(listPagesExplicitBrowse).toHaveBeenCalledWith("active", undefined, 500, 0);
    expect(listPagesExplicitBrowse).toHaveBeenCalledWith("draft", undefined, 500, 0);
    for (const hiddenName of ["Cached active name", "Cached draft name", "Passive candidate name", "candidate content"]) {
      expect(screen.queryByText(hiddenName)).not.toBeInTheDocument();
    }
    expect(container.querySelector("table")).toBeNull();
    expect(container.querySelector(".asset-cards, [data-testid='wiki-cards'], [data-testid='pages-library']")).toBeNull();
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(distillReview).not.toHaveBeenCalled();
    expect(listRefinements).not.toHaveBeenCalled();
  });

});
