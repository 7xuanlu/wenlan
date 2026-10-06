// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ComponentProps } from "react";
import { i18n } from "../../i18n";
import type { Page } from "../../lib/tauri";
import Sidebar, { SidebarToggleButton } from "./Sidebar";

const { listAllActivePagesMock, listAllDraftPagesMock } = vi.hoisted(() => ({
  listAllActivePagesMock: vi.fn().mockResolvedValue([]),
  listAllDraftPagesMock: vi.fn().mockResolvedValue([]),
}));

vi.mock("./pages/listAllPages", () => ({
  listAllActivePages: listAllActivePagesMock,
  listAllDraftPages: listAllDraftPagesMock,
}));
vi.mock("./IdentityCard", () => ({
  default: ({ onOpenAbout, onOpenDetail, onOpenSettings }: {
    readonly onOpenAbout?: () => void;
    readonly onOpenDetail: (entityId: string) => void;
    readonly onOpenSettings?: () => void;
  }) => (
    <div data-testid="identity-card">
      <button onClick={() => onOpenDetail("person-1")} type="button">Open identity detail</button>
      <button onClick={onOpenSettings} type="button">Open identity settings</button>
      <button onClick={onOpenAbout} type="button">Open identity about</button>
    </div>
  ),
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

function renderSidebar(extraProps: Partial<ComponentProps<typeof Sidebar>> = {}) {
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
        onNavigatePages={() => {}}
        onNavigateSpaces={() => {}}
        onSelectPage={() => {}}
        {...extraProps}
      />
    </QueryClientProvider>,
  );
}

describe("Sidebar workspace", () => {
  beforeEach(async () => {
    listAllActivePagesMock.mockReset().mockResolvedValue([]);
    listAllDraftPagesMock.mockReset().mockResolvedValue([]);
    localStorage.clear();
    await i18n.changeLanguage("en");
  });

  it("gives the default desktop workspace a 48px rail and 216px page panel", () => {
    const { container } = renderSidebar();
    const aside = screen.getByRole("complementary", { name: "Primary navigation" });
    expect(aside).toHaveStyle({ width: "264px" });
    expect(container.querySelector(".notes-icon-rail")).toBeInTheDocument();
    expect(container.querySelector(".notes-workspace-panel")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Wiki" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Spaces" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Graph" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Home" })).not.toBeInTheDocument();
  });

  it("keeps the rail operable when the desktop page list is collapsed", async () => {
    const user = userEvent.setup();
    const onNavigatePages = vi.fn();
    renderSidebar({ collapsed: true, open: false, onNavigatePages });

    const aside = screen.getByRole("complementary", { name: "Primary navigation" });
    expect(aside).toHaveStyle({ width: "48px" });
    expect(aside).not.toHaveAttribute("aria-hidden", "true");
    expect(screen.queryByRole("region", { name: "Notes" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Wiki" }));
    expect(onNavigatePages).toHaveBeenCalledTimes(1);
  });

  it("omits Home from More and customization even when a legacy callback is supplied", async () => {
    const user = userEvent.setup();
    const onNavigateHome = vi.fn();
    renderSidebar({ onNavigateHome, onNavigateSources: vi.fn(), onNavigateEntities: vi.fn() });
    await user.click(screen.getByRole("button", { name: "More" }));
    expect(screen.queryByRole("button", { name: "Home" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Customize navigation" }));
    expect(screen.getAllByRole("checkbox")).toHaveLength(6);
    expect(screen.queryByRole("checkbox", { name: "Home" })).not.toBeInTheDocument();
    expect(onNavigateHome).not.toHaveBeenCalled();
  });

  it("routes active pages, drafts and creation through the supplied Main callbacks", async () => {
    const active = page("active-1", "Project plan");
    const draft = page("draft-1", "Rough note", "draft");
    draft.space = "Work";
    listAllActivePagesMock.mockResolvedValue([active]);
    listAllDraftPagesMock.mockResolvedValue([draft]);
    const user = userEvent.setup();
    const onSelectPage = vi.fn();
    const onSelectDraft = vi.fn();
    const onCreatePage = vi.fn();
    renderSidebar({ onSelectPage, onSelectDraft, onCreatePage });

    await user.click(await screen.findByRole("button", { name: "Expand Other notes" }));
    await user.click(screen.getByRole("button", { name: "Open Project plan" }));
    await user.click(screen.getByRole("button", { name: "Expand Drafts" }));
    await user.click(screen.getByRole("button", { name: "Open Rough note" }));
    await user.click(screen.getByRole("button", { name: "New note" }));
    expect(onSelectPage).toHaveBeenCalledWith(active);
    expect(onSelectDraft).toHaveBeenCalledWith("draft-1", "Work");
    expect(onCreatePage).toHaveBeenCalledTimes(1);
  });

  it("keeps Sources in More and Settings directly in the rail", async () => {
    const user = userEvent.setup();
    const onNavigateSources = vi.fn();
    const onNavigateSettings = vi.fn();
    renderSidebar({ onNavigateSources, onNavigateSettings, activeNavigation: "sources" });

    const more = screen.getByRole("button", { name: "More" });
    expect(more).toHaveAttribute("aria-current", "page");
    await user.click(more);
    await user.click(screen.getByRole("button", { name: "Sources", current: "page" }));
    expect(onNavigateSources).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Settings" }).closest(".notes-rail-utilities")).not.toBeNull();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(onNavigateSettings).toHaveBeenCalledTimes(1);
  });

  it("closes the narrow drawer after navigation from More and the page list", async () => {
    const active = page("page-1", "One note");
    listAllActivePagesMock.mockResolvedValue([active]);
    const user = userEvent.setup();
    const onRequestClose = vi.fn();
    const onSelectPage = vi.fn();
    const onNavigateGraph = vi.fn();
    renderSidebar({
      open: true,
      presentation: "overlay",
      onRequestClose,
      onSelectPage,
      onNavigateGraph,
    });

    await user.click(await screen.findByRole("button", { name: "Expand Other notes" }));
    await user.click(screen.getByRole("button", { name: "Open One note" }));
    await user.click(screen.getByRole("button", { name: "More" }));
    await user.click(screen.getByRole("button", { name: "Graph" }));
    expect(onSelectPage).toHaveBeenCalledWith(active);
    expect(onNavigateGraph).toHaveBeenCalledTimes(1);
    expect(onRequestClose).toHaveBeenCalledTimes(2);
  });

  it("keeps the closed narrow drawer out of the accessibility tree", () => {
    renderSidebar({ collapsed: true, open: false, presentation: "overlay" });
    expect(screen.queryByRole("button", { name: "Wiki" })).not.toBeInTheDocument();
  });

  it("keeps account controls in the rail even when the note list is collapsed", async () => {
    const user = userEvent.setup();
    const onOpenAbout = vi.fn();
    renderSidebar({ onOpenAbout, collapsed: true, open: false });
    await user.click(screen.getByRole("button", { name: "Open identity about" }));
    expect(onOpenAbout).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("identity-card").closest(".notes-rail-utilities")).not.toBeNull();
  });

  it("keeps the fixed-size header toggle named in both states", () => {
    const { rerender } = render(<SidebarToggleButton collapsed={false} onToggle={() => {}} />);
    expect(screen.getByRole("button", { name: "Hide sidebar" })).toHaveStyle({ width: "28px", height: "28px" });
    rerender(<SidebarToggleButton collapsed onToggle={() => {}} />);
    expect(screen.getByRole("button", { name: "Show sidebar" })).toHaveAttribute("data-sidebar-toggle", "true");
  });
});
