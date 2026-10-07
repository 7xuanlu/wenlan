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
  listOnboardingMilestones: vi.fn().mockResolvedValue([]),
  getTruthStatus: vi.fn().mockResolvedValue(null),
  listSpaces: vi.fn(),
  knowledgeFoldersList: vi.fn().mockResolvedValue({ folders: [], truncated: false }),
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
  onOpenReview = vi.fn(),
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
          onOpenReview={onOpenReview}
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
    window.localStorage.removeItem("wenlan-wiki-view-mode");
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
    window.localStorage.setItem("wenlan-wiki-view-mode", "rows");
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

    expect(screen.getByRole("button", {
      name: "Open Needs verification · Needs review",
    })).toBeInTheDocument();
    expect(screen.getByRole("button", {
      name: "Open Working theory · Draft",
    })).toBeInTheDocument();
  });

  it("renders the approved full-width Wiki inventory without inventing a label for empty Space", async () => {
    window.localStorage.setItem("wenlan-wiki-view-mode", "rows");
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "independent", space: null }),
      page({ id: "entity", title: "Nash Su", entity_id: "entity-1", space: "Research" }),
      page({ id: "decision", title: "Why citations stay visible", content: "Decision: keep citations visible.", space: "Wenlan" }),
      page({ id: "recap", title: "July research recap", space: "Research" }),
    ]);
    const user = userEvent.setup();
    const { onSelectPage } = renderOverview();

    expect(await screen.findByRole("heading", { name: "Wiki" })).toBeInTheDocument();
    expect(screen.queryByText("A living ledger of ideas, people, decisions, and recaps.")).not.toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "All pages" })).not.toBeInTheDocument();
    expect(await screen.findByText("3 pages")).toBeInTheDocument();
    for (const heading of ["Page", "Space", "Updated"]) {
      expect(screen.getByRole("columnheader", { name: heading })).toBeInTheDocument();
    }
    expect(screen.queryByRole("columnheader", { name: "Kind" })).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
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
    renderOverview();

    expect(await screen.findByRole("button", { name: "Open Needs verification · Needs review" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Open Nash Su/ })).toBeNull();

    expect(await screen.findByRole("button", { name: "Open Needs verification · Needs review" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Open Nash Su/ })).toBeNull();
  });

  it("treats persisted unconfirmed Pages as an inventory status, not a new-page candidate", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "confirmed", title: "Confirmed note" }),
      page({ id: "unconfirmed", title: "Needs verification", review_status: "unconfirmed" }),
    ]);
    renderOverview();

    await screen.findByRole("button", { name: "Open Needs verification · Needs review" });
    expect(screen.getByText("Needs review", { selector: "span" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "New page candidates" })).not.toBeInTheDocument();

    expect(screen.getByRole("button", { name: "Open Needs verification · Needs review" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Confirmed note" })).toBeInTheDocument();
  });

  it("marks a persisted Page when Review has a page cleanup suggestion", async () => {
    window.localStorage.setItem("wenlan-wiki-view-mode", "rows");
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
    window.localStorage.setItem("wenlan-wiki-view-mode", "rows");
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
    window.localStorage.setItem("wenlan-wiki-view-mode", "rows");
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

  it("keeps Review opt-in behind page options, with keyboard dismissal", async () => {
    const user = userEvent.setup();
    const onOpenReview = vi.fn();
    renderOverview({ onOpenReview });
    const options = await screen.findByRole("button", { name: "Page options" });
    expect(screen.queryByRole("button", { name: "Review page changes" })).not.toBeInTheDocument();
    await user.click(options);
    expect(screen.getByRole("menuitem", { name: "Review page changes" })).toHaveFocus();
    expect(onOpenReview).not.toHaveBeenCalled();
    expect(distillReview).not.toHaveBeenCalled();
    await user.keyboard("{Escape}");
    expect(options).toHaveFocus();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    await user.keyboard("{Enter}");
    await user.keyboard("{Enter}");
    expect(onOpenReview).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("paginates twelve notes and keeps the Wiki controls quiet", async () => {
    window.localStorage.setItem("wenlan-wiki-view-mode", "rows");
    vi.mocked(listPagesExplicitBrowse).mockImplementation(async status => status === "draft" ? [] : Array.from({ length: 14 }, (_, index) => page({ id: `p-${index}`, title: `Note ${index}` })));
    renderOverview();
    await screen.findByRole("button", { name: "Open Note 0" });
    expect(screen.getAllByRole("row")).toHaveLength(13);
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: /Next/ }));
    expect(screen.getAllByRole("row")).toHaveLength(3);
  });

  it("shows a quiet empty state without inventing a Space requirement", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([]);
    renderOverview();

    expect(await screen.findByText("No pages yet")).toBeInTheDocument();
    expect(screen.queryByText("Create a space")).not.toBeInTheDocument();
    expect(screen.getByText("Write your first note. Add AI help when you need it.")).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "View" })).not.toBeInTheDocument();
  });

  it("renders cards by default with title, summary, space chip, and time", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "independent", space: null }),
      page({ id: "recap", title: "July research recap", space: "Research" }),
    ]);
    const user = userEvent.setup();
    const { onSelectSpace } = renderOverview();

    const cards = await screen.findByTestId("wiki-cards");
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(within(cards).getAllByTestId(/^wiki-card-/)).toHaveLength(2);

    const recap = screen.getByTestId("wiki-card-recap");
    expect(within(recap).getByText("July research recap")).toBeInTheDocument();
    expect(within(recap).getByText("A page can stand on its own.")).toBeInTheDocument();
    const time = recap.querySelector("time");
    expect(time).not.toBeNull();
    expect(time?.getAttribute("dateTime")).toBeTruthy();

    await user.click(within(recap).getByRole("button", { name: "Open Space: Research" }));
    expect(onSelectSpace).toHaveBeenCalledWith("Research");
  });

  it("switching to rows shows the table and persists the preference", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "independent", space: null }),
    ]);
    const user = userEvent.setup();
    renderOverview();

    expect(await screen.findByTestId("wiki-cards")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "List" }));

    expect(await screen.findByRole("columnheader", { name: "Page" })).toBeInTheDocument();
    expect(screen.queryByTestId("wiki-cards")).not.toBeInTheDocument();
    expect(window.localStorage.getItem("wenlan-wiki-view-mode")).toBe("rows");
    expect(screen.getByRole("button", { name: "List" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Cards" })).toHaveAttribute("aria-pressed", "false");
  });

  it("restores the stored rows lens on mount", async () => {
    window.localStorage.setItem("wenlan-wiki-view-mode", "rows");
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "independent", space: null }),
    ]);
    renderOverview();

    expect(await screen.findByRole("columnheader", { name: "Page" })).toBeInTheDocument();
    expect(screen.queryByTestId("wiki-cards")).not.toBeInTheDocument();
  });

  it("opens pages and drafts from cards exactly like the rows do", async () => {
    vi.mocked(listPagesExplicitBrowse).mockImplementation(async (status) => status === "draft"
      ? [
          page({
            id: "draft-titled",
            title: "Working theory",
            status: "draft",
            review_status: "unconfirmed",
            space: "Research",
          }),
        ]
      : [page({ id: "active", title: "Published note" })]);
    const user = userEvent.setup();
    const onSelectDraft = vi.fn();
    const onSelectPage = vi.fn();
    renderOverview({ onSelectDraft, onSelectPage });

    await user.click(await screen.findByRole("button", { name: "Open Published note" }));
    expect(onSelectPage).toHaveBeenCalledWith("active");

    await user.click(screen.getByRole("button", { name: "Open Working theory · Draft" }));
    expect(onSelectDraft).toHaveBeenCalledWith("draft-titled", "Research");
  });

  it("renders no context node when a page has no summary", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue([
      page({ id: "bare", title: "Bare note", summary: null }),
    ]);
    renderOverview();

    const card = await screen.findByTestId("wiki-card-bare");
    expect(within(card).getByText("Bare note")).toBeInTheDocument();
    expect(card.querySelector(".asset-card-context")).toBeNull();
  });

  it("keeps identical pagination numbers across lenses", async () => {
    vi.mocked(listPagesExplicitBrowse).mockResolvedValue(
      Array.from({ length: 13 }, (_, index) => page({
        id: `topic-${index}`,
        title: `Topic ${index}`,
        last_modified: `2026-07-${String(index + 1).padStart(2, "0")}T00:00:00Z`,
      })),
    );
    const user = userEvent.setup();
    renderOverview();

    expect(await screen.findByText("1–12 of 13")).toBeInTheDocument();
    expect(screen.getAllByTestId(/^wiki-card-/)).toHaveLength(12);

    await user.click(screen.getByRole("button", { name: "List" }));
    expect(await screen.findByText("1–12 of 13")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^Open / })).toHaveLength(12);

    await user.click(screen.getByRole("button", { name: "Next" }));
    expect(await screen.findByText("13–13 of 13")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Cards" }));
    expect(await screen.findByText("13–13 of 13")).toBeInTheDocument();
    expect(screen.getAllByTestId(/^wiki-card-/)).toHaveLength(1);
  });
});
