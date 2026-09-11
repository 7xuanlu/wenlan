import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  distillReview,
  listPagesExplicitBrowse,
  listRefinements,
  listSpaces,
  type DistillReviewResponse,
  type Page,
} from "../../../lib/tauri";
import { PagesOverview } from "./PagesOverview";
import { DISTILL_REVIEW_SESSION_QUERY_KEY } from "./pageReviewSignals";

vi.mock("../../../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../lib/tauri")>()),
  distillReview: vi.fn(),
  listPagesExplicitBrowse: vi.fn(),
  listRefinements: vi.fn(),
  listSpaces: vi.fn(),
}));

function page(overrides: Partial<Page>): Page {
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
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
} = {}) {
  return {
    queryClient,
    onCreatePage,
    onSelectDraft,
    onSelectPage,
    onSelectSpace,
    ...render(
      <QueryClientProvider client={queryClient}>
        <PagesOverview
          onCreatePage={onCreatePage}
          onSelectDraft={onSelectDraft}
          onSelectPage={onSelectPage}
          onSelectSpace={onSelectSpace}
        />
      </QueryClientProvider>,
    ),
  };
}

describe("PagesOverview", () => {
  beforeEach(() => {
    vi.mocked(listPagesExplicitBrowse).mockReset();
    vi.mocked(listRefinements).mockReset();
    vi.mocked(listRefinements).mockResolvedValue({ proposals: [] });
    vi.mocked(distillReview).mockReset();
    vi.mocked(listSpaces).mockReset();
    vi.mocked(listSpaces).mockResolvedValue([]);
  });

  it("opens a standalone Page editor directly from the Wiki header", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([]);
    const user = userEvent.setup();
    const { onCreatePage } = renderOverview();

    const newPage = await screen.findByRole("button", { name: "New page" });
    await user.click(newPage);

    expect(onCreatePage).toHaveBeenCalledWith(null);
    expect(screen.queryByRole("dialog", { name: "New page" })).not.toBeInTheDocument();
  });

  it("combines active and draft inventories without treating a draft as needing review", async () => {
    vi.mocked(listPagesExplicitBrowse).mockImplementation(async (status) => status === "draft"
      ? [
          page({
            id: "draft-titled",
            title: "Working theory",
            status: "draft",
            review_status: "unconfirmed",
            space: "Research",
          }),
          page({
            id: "draft-untitled",
            title: "",
            status: "draft",
            review_status: "unconfirmed",
          }),
        ]
      : [
          page({ id: "active", title: "Published note" }),
          page({
            id: "active-unconfirmed",
            title: "Needs verification",
            review_status: "unconfirmed",
          }),
        ]);
    const user = userEvent.setup();
    const onSelectDraft = vi.fn();
    const onSelectPage = vi.fn();
    renderOverview({ onSelectDraft, onSelectPage });

    expect(await screen.findByText("4 pages")).toBeInTheDocument();
    const draftAction = screen.getByRole("button", { name: "Open Working theory · Draft" });
    const draftRow = draftAction.closest("tr");
    expect(draftRow).not.toBeNull();
    expect(within(draftRow!).getByText("Draft")).toBeInTheDocument();
    expect(within(draftRow!).queryByText("Needs review")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Untitled draft · Draft" })).toBeInTheDocument();

    fireEvent.click(draftRow!);
    expect(onSelectDraft).toHaveBeenCalledWith("draft-titled", "Research");
    expect(onSelectPage).not.toHaveBeenCalled();

    await user.selectOptions(screen.getByRole("combobox", { name: "State" }), "unconfirmed");
    expect(screen.getByRole("button", {
      name: "Open Needs verification · Needs review",
    })).toBeInTheDocument();
    expect(screen.queryByRole("button", {
      name: "Open Working theory · Draft",
    })).not.toBeInTheDocument();
  });

  it("renders the approved full-width Wiki inventory without inventing a label for empty Space", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "independent", space: null }),
      page({ id: "entity", title: "Nash Su", entity_id: "entity-1", space: "Research" }),
      page({ id: "decision", title: "Why citations stay visible", content: "Decision: keep citations visible.", space: "Wenlan" }),
      page({ id: "recap", title: "July research recap", space: "Research" }),
    ]);
    const user = userEvent.setup();
    const { onSelectPage } = renderOverview();

    expect(await screen.findByRole("heading", { name: "Wiki" })).toBeInTheDocument();
    expect(screen.getByText("A living ledger of ideas, people, decisions, and recaps.")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "All pages" })).not.toBeInTheDocument();
    expect(await screen.findByText("3 pages")).toBeInTheDocument();
    for (const heading of ["Page", "Space", "Updated"]) {
      expect(screen.getByRole("columnheader", { name: heading })).toBeInTheDocument();
    }
    expect(screen.queryByRole("columnheader", { name: "Kind" })).not.toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Space" })).toHaveValue("all");
    expect(screen.getByRole("combobox", { name: "Sort" })).toHaveValue("recent");
    expect(screen.getByTestId("page-space-independent")).toBeEmptyDOMElement();
    for (const rejected of ["Independent", "Optional", "Unassigned", "No Space", "Browse by type", "Independent pages", "In spaces"]) {
      expect(screen.queryByText(rejected)).not.toBeInTheDocument();
    }
    expect(listPagesExplicitBrowse).toHaveBeenCalledWith("active", undefined, 500, 0);

    await user.click(screen.getByRole("button", { name: "Open Independent research note" }));
    expect(onSelectPage).toHaveBeenCalledWith("independent");

    expect(screen.queryByRole("button", { name: /Open Nash Su/ })).toBeNull();
  });

  it("never lists an established entity row in the Wiki (#708)", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "entity", title: "Nash Su", entity_id: "entity-1", review_status: "unconfirmed" }),
      page({ id: "prose", title: "Needs verification", review_status: "unconfirmed" }),
    ]);
    const user = userEvent.setup();
    renderOverview();

    expect(await screen.findByRole("button", { name: "Open Needs verification · Needs review" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Open Nash Su/ })).toBeNull();

    await user.selectOptions(screen.getByRole("combobox", { name: "State" }), "unconfirmed");
    expect(await screen.findByRole("button", { name: "Open Needs verification · Needs review" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Open Nash Su/ })).toBeNull();
  });

  it("treats persisted unconfirmed Pages as an inventory status, not a new-page candidate", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "confirmed", title: "Confirmed note" }),
      page({ id: "unconfirmed", title: "Needs verification", review_status: "unconfirmed" }),
    ]);
    const user = userEvent.setup();
    renderOverview();

    const statusFilter = await screen.findByRole("combobox", { name: "State" });
    expect(screen.getByText("Needs review")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "New page candidates" })).not.toBeInTheDocument();

    await user.selectOptions(statusFilter, "unconfirmed");
    expect(screen.getByRole("button", { name: "Open Needs verification · Needs review" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open Confirmed note" })).not.toBeInTheDocument();
  });

  it("marks a persisted Page when Review has a page cleanup suggestion", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "thin-page", title: "Thin research note" }),
    ]);
    vi.mocked(listRefinements).mockResolvedValue({
      proposals: [
        {
          id: "proposal-1",
          action: "page_keep_or_archive",
          source_ids: ["thin-page"],
          payload: {
            action: "page_keep_or_archive",
            page_id: "thin-page",
            source_count: 1,
          },
          confidence: 0.92,
          created_at: "2026-07-16 00:00:00",
        },
      ],
    });
    renderOverview();

    const row = (await screen.findByRole("button", { name: "Open Thin research note · Cleanup suggested" })).closest("tr");
    expect(row).not.toBeNull();
    expect(await within(row!).findByText("Cleanup suggested")).toBeInTheDocument();
    expect(listRefinements).toHaveBeenCalledWith(50);
  });

  it("includes every visible persisted state in the Page action name without changing row navigation", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({
        id: "review-page",
        title: "Review boundary",
        review_status: "unconfirmed",
      }),
    ]);
    vi.mocked(listRefinements).mockResolvedValue({
      proposals: [
        {
          id: "proposal-review-page",
          action: "page_keep_or_archive",
          source_ids: ["memory-evidence"],
          payload: {
            action: "page_keep_or_archive",
            page_id: "review-page",
            source_count: 1,
          },
          confidence: 0.92,
          created_at: "2026-07-16 00:00:00",
        },
      ],
    });
    const onSelectPage = vi.fn();
    renderOverview({ onSelectPage });

    const pageAction = await screen.findByRole("button", {
      name: "Open Review boundary · Needs review · Cleanup suggested",
    });
    const row = pageAction.closest("tr");
    expect(row).not.toBeNull();
    expect(within(row!).getByText("Needs review")).toBeInTheDocument();
    expect(within(row!).getByText("Cleanup suggested")).toBeInTheDocument();

    fireEvent.click(row!);
    expect(onSelectPage).toHaveBeenCalledWith("review-page");
  });

  it("does not treat cleanup evidence memory ids as Page ids", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "memory-evidence", title: "Ordinary page" }),
    ]);
    vi.mocked(listRefinements).mockResolvedValue({
      proposals: [
        {
          id: "proposal-without-page-payload",
          action: "page_keep_or_archive",
          source_ids: ["memory-evidence"],
          payload: null,
          confidence: 0.92,
          created_at: "2026-07-16 00:00:00",
        },
      ],
    });
    renderOverview();

    const row = (await screen.findByRole("button", { name: "Open Ordinary page" })).closest("tr");
    expect(row).not.toBeNull();
    expect(within(row!).queryByText("Cleanup suggested")).not.toBeInTheDocument();
  });

  it("shows only cached page candidates, routes linked candidates, and previews or hides unlinked candidates", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([page({ id: "page-existing", title: "Temporal page refresh" })]);
    const discovery: DistillReviewResponse = {
      pages_created: 0,
      scoped: false,
      created_ids: [],
      pending: [
        {
          source_ids: ["memory-linked"],
          contents: ["Existing page source"],
          entity_name: "Temporal page refresh",
          estimated_tokens: 40,
          existing_page_id: "page-existing",
          existing_page_title: "Temporal page refresh",
        },
        {
          source_ids: ["memory-new"],
          contents: ["A new cluster waiting for the next compile pass."],
          entity_name: "Vector clocks",
          estimated_tokens: 55,
        },
      ],
      stale_pages: [],
      stale_truncated: false,
      orphan_topics: [],
    };
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(DISTILL_REVIEW_SESSION_QUERY_KEY, discovery);
    const user = userEvent.setup();
    const onSelectPage = vi.fn();
    renderOverview({ queryClient, onSelectPage });

    expect(await screen.findByRole("heading", { name: "New page candidates" })).toBeInTheDocument();
    const linkedCandidate = screen.getByRole("button", { name: "Open page: Temporal page refresh" });
    expect(within(linkedCandidate).getByText("1 source")).toBeInTheDocument();
    await user.click(linkedCandidate);
    expect(onSelectPage).toHaveBeenCalledWith("page-existing");

    await user.click(screen.getByRole("button", { name: "Preview candidate: Vector clocks" }));
    expect(await screen.findByText("A new cluster waiting for the next compile pass.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Hide" }));
    expect(screen.queryByRole("button", { name: "Preview candidate: Vector clocks" })).not.toBeInTheDocument();
  });

  it("keeps explicit Review candidates after leaving Wiki for longer than the default query GC window", () => {
    vi.useFakeTimers();
    try {
      vi.mocked(listPagesExplicitBrowse).mockResolvedValue([]);
      const discovery: DistillReviewResponse = {
        pages_created: 0,
        scoped: false,
        created_ids: [],
        pending: [
          {
            source_ids: ["memory-delayed"],
            contents: ["A candidate discovered by the user's explicit Review run."],
            entity_name: "Delayed navigation candidate",
            estimated_tokens: 45,
          },
        ],
        stale_pages: [],
        stale_truncated: false,
        orphan_topics: [],
      };
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false } },
      });
      queryClient.setQueryData(DISTILL_REVIEW_SESSION_QUERY_KEY, discovery);

      const firstVisit = renderOverview({ queryClient });
      expect(
        screen.getByRole("button", {
          name: "Preview candidate: Delayed navigation candidate",
        }),
      ).toBeInTheDocument();
      firstVisit.unmount();

      act(() => {
        vi.advanceTimersByTime(5 * 60_000 + 1);
      });
      expect(queryClient.getQueryData(DISTILL_REVIEW_SESSION_QUERY_KEY)).toEqual(
        discovery,
      );

      const returnVisit = renderOverview({ queryClient });
      expect(
        screen.getByRole("button", {
          name: "Preview candidate: Delayed navigation candidate",
        }),
      ).toBeInTheDocument();
      returnVisit.unmount();
    } finally {
      vi.useRealTimers();
    }
  });

  it("never starts a distill review when Wiki mounts", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([page({ title: "Quiet Wiki mount" })]);
    renderOverview();

    expect(await screen.findByRole("button", { name: "Open Quiet Wiki mount" })).toBeInTheDocument();
    expect(distillReview).not.toHaveBeenCalled();
  });

  it("filters by Space, sorts by title, and paginates seven rows at a time", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "topic-z", title: "Zulu topic", space: null }),
      page({ id: "decision", title: "Why citations stay visible", content: "Decision: keep citations visible.", space: "Wenlan" }),
      page({ id: "recap", title: "July research recap", space: "Research" }),
      ...Array.from({ length: 5 }, (_, index) => page({ id: `topic-${index}`, title: `Topic ${index}`, last_modified: `2026-07-0${index + 1}T00:00:00Z` })),
    ]);
    const user = userEvent.setup();
    renderOverview();

    expect(await screen.findByText("1–7 of 8")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open Topic 0" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(await screen.findByText("8–8 of 8")).toBeInTheDocument();

    await user.selectOptions(screen.getByRole("combobox", { name: "Space" }), "Research");
    expect(screen.getByRole("button", { name: "Open July research recap" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open Why citations stay visible" })).not.toBeInTheDocument();

    await user.selectOptions(screen.getByRole("combobox", { name: "Space" }), "all");
    await user.selectOptions(screen.getByRole("combobox", { name: "Sort" }), "title");
    const openButtons = screen.getAllByRole("button", { name: /^Open / });
    expect(openButtons[0]).toHaveAccessibleName("Open July research recap");
  });

  it("shows a quiet empty state without inventing a Space requirement", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([]);
    renderOverview();

    expect(await screen.findByText("No pages yet")).toBeInTheDocument();
    expect(screen.queryByText("Create a space")).not.toBeInTheDocument();
  });
});
