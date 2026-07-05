// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ComponentProps } from "react";
import { i18n } from "../../i18n";
import Main from "./Main";

const eventListeners = vi.hoisted(
  () => new Map<string, (payload?: unknown) => void>(),
);

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: (payload?: unknown) => void) => {
    eventListeners.set(event, handler);
    return Promise.resolve(() => eventListeners.delete(event));
  }),
}));

vi.mock("../../hooks/useSearch", () => ({
  useSearch: () => ({
    query: "",
    setQuery: vi.fn(),
    results: [],
  }),
}));

vi.mock("../../lib/tauri", () => ({
  listMemoriesRich: vi.fn().mockResolvedValue([]),
  getMemoryStats: vi.fn().mockResolvedValue({ total: 0, new_today: 0, confirmed: 0, domains: [] }),
  searchEntities: vi.fn().mockResolvedValue([]),
  searchPages: vi.fn().mockResolvedValue([]),
  deleteFileChunks: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("./ActivityFeed", () => ({ default: () => <div /> }));
vi.mock("./IdentityDetail", () => ({ default: () => <div /> }));
vi.mock("./MemoryStream", () => ({ default: () => <div /> }));
vi.mock("./HomePage", () => ({ default: () => <div data-testid="home-page" /> }));
vi.mock("./ConstellationMap", () => ({ default: () => <div /> }));
vi.mock("./MemoryStatusBar", () => ({ default: () => <div /> }));
vi.mock("./MemorySearchResult", () => ({ default: () => <div /> }));
vi.mock("./MemoryDetail", () => ({ default: () => <div data-testid="memory-detail" /> }));
vi.mock("./PageDetail", () => ({ default: () => <div /> }));
vi.mock("./DistillReviewPanel", () => ({ default: () => <div /> }));
vi.mock("./SettingsPage", () => ({ default: () => <div data-testid="settings-page" /> }));
vi.mock("../SetupWizard", () => ({ SetupWizard: () => <div /> }));
vi.mock("../ViewToggle", () => ({ default: () => <div /> }));
vi.mock("./Sidebar", () => ({
  default: (props: { onEntityClick: (entityId: string) => void }) => (
    <aside>
      <button type="button" onClick={() => props.onEntityClick("__create_profile__")}>
        Open avatar menu destination
      </button>
    </aside>
  ),
  SidebarToggleButton: () => <button type="button" aria-label="Toggle sidebar" />,
}));
vi.mock("./settings/SettingsSidebar", () => ({ default: () => <aside /> }));
vi.mock("./SpaceDetail", () => ({ default: () => <div /> }));
vi.mock("./DecisionLog", () => ({ default: () => <div /> }));
vi.mock("./MemoryCard", () => ({ default: () => <div /> }));
vi.mock("./ImportView", () => ({ ImportView: () => <div /> }));

function renderMain(props: ComponentProps<typeof Main> = {}) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <Main {...props} />
    </QueryClientProvider>,
  );
}

describe("Main search", () => {
  beforeEach(async () => {
    eventListeners.clear();
    await i18n.changeLanguage("en");
  });

  it("keeps the Wenlan brand out of the top chrome", () => {
    renderMain();

    expect(screen.queryByAltText("Wenlan")).toBeNull();
    expect(screen.queryByRole("button", { name: "Go to home" })).toBeNull();
  });

  it("keeps Settings out of the top chrome because the account menu owns it", () => {
    renderMain();

    expect(screen.queryByRole("button", { name: "Settings" })).toBeNull();
  });

  it("labels the global search around wiki pages, entities, and sources", () => {
    renderMain();

    expect(screen.getByPlaceholderText("Search pages, entities, sources...")).toBeInTheDocument();
  });

  it("routes the former profile sentinel to General settings instead of the old profile page", async () => {
    const user = userEvent.setup();
    renderMain();

    await user.click(screen.getByRole("button", { name: "Open avatar menu destination" }));

    expect(await screen.findByTestId("settings-page")).toBeInTheDocument();
    expect(screen.queryByTestId("profile-page")).not.toBeInTheDocument();
  });

  it("focuses search from the Tauri event when the placeholder is translated", async () => {
    await i18n.changeLanguage("zh-Hant");
    renderMain();

    const searchInput = screen.getByPlaceholderText(
      "搜尋頁面、實體、來源...",
    );
    eventListeners.get("focus-search")?.();

    expect(searchInput).toHaveFocus();
  });
});
