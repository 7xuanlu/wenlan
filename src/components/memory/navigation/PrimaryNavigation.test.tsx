import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { PrimaryNavigation } from "./PrimaryNavigation";
import { NAVIGATION_DESTINATION_ORDER, NAVIGATION_PREFERENCE_KEY, writeNavigationPreferences } from "./navigationPreferences";
import type { GlobalNavigation } from "./viewState";

const labels = {
  backToMore: "Back to More",
  customizationHint: "Choose what stays in the sidebar.",
  customize: "Customize navigation",
  resetNavigation: "Reset defaults",
  entities: "Topics",
  graph: "Graph",
  memories: "Memories",
  more: "More",
  navigation: "Primary navigation",
  pages: "Wiki",
  sources: "Sources",
  spaces: "Spaces",
} as const;

function renderNavigation(active: GlobalNavigation | null = null) {
  const callbacks = {
    entities: vi.fn(), graph: vi.fn(), memories: vi.fn(),
    pages: vi.fn(), sources: vi.fn(), spaces: vi.fn(),
  };
  const props = {
    active, labels,
    onNavigateEntities: callbacks.entities,
    onNavigateGraph: callbacks.graph,
    onNavigateLog: callbacks.memories,
    onNavigatePages: callbacks.pages,
    onNavigateSources: callbacks.sources,
    onNavigateSpaces: callbacks.spaces,
  };
  return { callbacks, props, ...render(<PrimaryNavigation {...props} />) };
}

function railLabels() {
  return screen.getAllByRole("button").filter((button) => button.classList.contains("notes-rail-button")).map((button) => button.getAttribute("aria-label"));
}

async function openCustomize(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "More" }));
  await user.click(screen.getByRole("button", { name: "Customize navigation" }));
  return screen.getByRole("group", { name: "Customize navigation" });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("PrimaryNavigation", () => {
  it("defaults to Wiki, Spaces, Graph and More in that order with working named destinations", async () => {
    const user = userEvent.setup();
    const { callbacks } = renderNavigation("pages");
    expect(screen.getByRole("navigation", { name: "Primary navigation" })).toBeInTheDocument();
    expect(railLabels()).toEqual(["Wiki", "Spaces", "Graph", "More"]);
    const wiki = screen.getByRole("button", { name: "Wiki", current: "page" });
    expect(wiki).toHaveAttribute("title", "Wiki");
    expect(wiki.querySelector('[data-navigation-icon="wiki-page"]')).toHaveAttribute("aria-hidden", "true");
    await user.click(wiki);
    await user.click(screen.getByRole("button", { name: "Spaces" }));
    await user.click(screen.getByRole("button", { name: "Graph" }));
    expect(callbacks.pages).toHaveBeenCalledOnce();
    expect(callbacks.spaces).toHaveBeenCalledExactlyOnceWith(false);
    expect(callbacks.graph).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "Sources" })).not.toBeInTheDocument();
  });

  it("lists every unpinned route once in More and keeps Customize reachable", async () => {
    const user = userEvent.setup();
    const { callbacks } = renderNavigation("entities");
    const more = screen.getByRole("button", { name: "More" });
    expect(more).toHaveAttribute("aria-current", "page");
    await user.click(more);
    const disclosure = screen.getByRole("group", { name: "More" });
    expect(within(disclosure).getAllByRole("button").map((button) => button.textContent)).toEqual(["Sources", "Memories", "Topics", "Customize navigation"]);
    expect(more).not.toHaveAttribute("aria-current");
    expect(screen.getByRole("button", { name: "Topics", current: "page" })).toBeInTheDocument();
    for (const [label, callback] of [
      ["Sources", callbacks.sources], ["Memories", callbacks.memories],
      ["Topics", callbacks.entities],
    ] as const) {
      if (more.getAttribute("aria-expanded") === "false") await user.click(more);
      await user.click(screen.getByRole("button", { name: label }));
      expect(callback).toHaveBeenCalledOnce();
      expect(more).toHaveAttribute("aria-expanded", "false");
    }
  });

  it("offers six content destinations without a legacy Home pin or customization control", async () => {
    localStorage.setItem(NAVIGATION_PREFERENCE_KEY, JSON.stringify({ version: 1, visible: ["home", "sources"] }));
    const user = userEvent.setup();
    renderNavigation("sources");
    expect(railLabels()).toEqual(["Sources", "More"]);
    await user.click(screen.getByRole("button", { name: "More" }));
    expect(screen.queryByRole("button", { name: "Home" })).not.toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "More" })).getAllByRole("button").map((button) => button.textContent)).toEqual(["Wiki", "Spaces", "Graph", "Memories", "Topics", "Customize navigation"]);
    await user.click(screen.getByRole("button", { name: "Customize navigation" }));
    const customization = screen.getByRole("group", { name: "Customize navigation" });
    expect(within(customization).getAllByRole("checkbox")).toHaveLength(6);
    expect(within(customization).queryByRole("checkbox", { name: "Home" })).not.toBeInTheDocument();
    await user.click(within(customization).getByRole("checkbox", { name: "Wiki" }));
    expect(JSON.parse(localStorage.getItem(NAVIGATION_PREFERENCE_KEY)!).visible).toEqual(["pages", "sources"]);
  });

  it("applies pinning immediately in canonical order, persists on remount and keeps unpinned navigation reachable", async () => {
    const user = userEvent.setup();
    const view = renderNavigation("sources");
    const customize = await openCustomize(user);
    const sources = within(customize).getByRole("checkbox", { name: "Sources" });
    expect(sources).not.toBeChecked();
    await user.click(sources);
    await user.click(within(customize).getByText("Memories", { selector: "span" }));
    expect(railLabels()).toEqual(["Wiki", "Spaces", "Graph", "Sources", "Memories", "More"]);
    expect(customize).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sources", current: "page" })).toBeInTheDocument();
    await user.click(within(customize).getByRole("checkbox", { name: "Wiki" }));
    expect(railLabels()).toEqual(["Spaces", "Graph", "Sources", "Memories", "More"]);
    expect(view.callbacks.sources).not.toHaveBeenCalled();
    view.unmount();
    const remounted = renderNavigation("sources");
    expect(railLabels()).toEqual(["Spaces", "Graph", "Sources", "Memories", "More"]);
    await user.click(screen.getByRole("button", { name: "Sources" }));
    expect(remounted.callbacks.sources).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "More" }));
    await user.click(screen.getByRole("button", { name: "Wiki" }));
    expect(remounted.callbacks.pages).toHaveBeenCalledOnce();
    expect(screen.queryByRole("group")).not.toBeInTheDocument();
  });

  it("allows all destinations to be hidden, then reset restores defaults without closing customization", async () => {
    const user = userEvent.setup();
    renderNavigation("pages");
    const customize = await openCustomize(user);
    expect(within(customize).getAllByRole("checkbox").map((checkbox) => checkbox.closest("label")?.textContent)).toEqual(["Wiki", "Spaces", "Graph", "Sources", "Memories", "Topics"]);
    expect(within(customize).queryByRole("checkbox", { name: "Settings" })).not.toBeInTheDocument();
    for (const checkbox of within(customize).getAllByRole("checkbox")) {
      if ((checkbox as HTMLInputElement).checked) await user.click(checkbox);
    }
    expect(railLabels()).toEqual(["More"]);
    expect(JSON.parse(localStorage.getItem(NAVIGATION_PREFERENCE_KEY)!)).toEqual({ version: 1, visible: [] });
    await user.click(screen.getByRole("button", { name: "Back to More" }));
    expect(screen.getByRole("button", { name: "Customize navigation" })).toHaveFocus();
    expect(within(screen.getByRole("group", { name: "More" })).getAllByRole("button")).toHaveLength(7);
    await user.click(screen.getByRole("button", { name: "Customize navigation" }));
    await user.click(screen.getByRole("button", { name: "Reset defaults" }));
    expect(railLabels()).toEqual(["Wiki", "Spaces", "Graph", "More"]);
    expect(screen.getByRole("group", { name: "Customize navigation" })).toBeInTheDocument();
  });

  it("recovers malformed persistence and retains More when a saved empty preference remounts", () => {
    localStorage.setItem(NAVIGATION_PREFERENCE_KEY, '{"version":1,"visible":["old-route"]}');
    const view = renderNavigation();
    expect(railLabels()).toEqual(["Wiki", "Spaces", "Graph", "More"]);
    view.unmount();
    writeNavigationPreferences([]);
    renderNavigation("pages");
    expect(railLabels()).toEqual(["More"]);
    expect(screen.getByRole("button", { name: "More", current: "page" })).toBeInTheDocument();
  });

  it("has exactly one active marker in rail, More, hidden list and customization as visibility changes", async () => {
    const user = userEvent.setup();
    const { container } = renderNavigation("pages");
    const marker = () => container.querySelectorAll('[data-primary-navigation-active-marker="true"]');
    expect(marker()).toHaveLength(1);
    const customize = await openCustomize(user);
    expect(marker()).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Wiki", current: "page" })).toBeInTheDocument();
    const checkbox = within(customize).getByRole("checkbox", { name: "Wiki" });
    await user.click(checkbox);
    expect(checkbox).toHaveFocus();
    expect(checkbox.closest("label")).toHaveAttribute("aria-current", "page");
    expect(marker()).toHaveLength(1);
    expect(screen.getByRole("button", { name: "More" })).not.toHaveAttribute("aria-current");
    await user.click(screen.getByRole("button", { name: "Back to More" }));
    expect(screen.getByRole("button", { name: "Wiki", current: "page" })).toHaveClass("notes-more-item");
    expect(marker()).toHaveLength(1);
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "More", current: "page" })).toBeInTheDocument();
    expect(marker()).toHaveLength(1);
  });

  it("supports keyboard toggles and Escape returns focus to More without navigation", async () => {
    const user = userEvent.setup();
    const { callbacks } = renderNavigation();
    await user.tab();
    expect(screen.getByRole("button", { name: "Wiki" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Spaces" })).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(callbacks.spaces).toHaveBeenCalledExactlyOnceWith(false);
    await openCustomize(user);
    expect(screen.getByRole("button", { name: "Back to More" })).toHaveFocus();
    const sources = screen.getByRole("checkbox", { name: "Sources" });
    sources.focus();
    await user.keyboard(" ");
    expect(sources).toBeChecked();
    expect(sources).toHaveFocus();
    await user.keyboard("{Escape}");
    const more = screen.getByRole("button", { name: "More" });
    expect(more).toHaveFocus();
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(callbacks.sources).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("group", { name: "More" })).toBeInTheDocument();
  });

  it("omits unwired routes from both lists and controls while preserving saved choices", async () => {
    const user = userEvent.setup();
    writeNavigationPreferences(NAVIGATION_DESTINATION_ORDER);
    const { rerender } = render(<PrimaryNavigation active={null} labels={labels} onNavigateSpaces={() => {}} />);
    expect(railLabels()).toEqual(["Spaces", "More"]);
    const customize = await openCustomize(user);
    expect(within(customize).getAllByRole("checkbox")).toHaveLength(1);
    await user.click(screen.getByRole("checkbox", { name: "Spaces" }));
    expect(JSON.parse(localStorage.getItem(NAVIGATION_PREFERENCE_KEY)!).visible).toEqual(NAVIGATION_DESTINATION_ORDER.filter((key) => key !== "spaces"));
    rerender(<PrimaryNavigation active={null} labels={labels} onNavigatePages={() => {}} onNavigateSpaces={() => {}} />);
    expect(railLabels()).toEqual(["Wiki", "More"]);
    await user.click(screen.getByRole("button", { name: "Back to More" }));
    expect(within(screen.getByRole("group", { name: "More" })).getAllByRole("button").map((button) => button.textContent)).toEqual(["Spaces", "Customize navigation"]);
  });

  it("keeps customization usable when preference storage is unavailable", async () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("disabled"); });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("disabled"); });
    const user = userEvent.setup();
    renderNavigation();
    const customize = await openCustomize(user);
    await user.click(within(customize).getByRole("checkbox", { name: "Sources" }));
    expect(railLabels()).toEqual(["Wiki", "Spaces", "Graph", "Sources", "More"]);
    expect(customize).toBeInTheDocument();
  });

  it("clamps a late More anchor inside a narrow viewport and updates after resize", async () => {
    const user = userEvent.setup();
    writeNavigationPreferences(NAVIGATION_DESTINATION_ORDER);
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("notes-more-anchor")
        ? { left: 5, right: 41, top: 338, bottom: 374, width: 36, height: 36, x: 5, y: 338, toJSON: () => ({}) }
        : { left: 48, right: 312, top: 338, bottom: 638, width: 264, height: 300, x: 48, y: 338, toJSON: () => ({}) };
    });
    vi.stubGlobal("innerWidth", 375);
    vi.stubGlobal("innerHeight", 400);
    renderNavigation();
    await user.click(screen.getByRole("button", { name: "More" }));
    const panel = screen.getByRole("group", { name: "More" });
    expect(panel).toHaveStyle({ left: "43px", top: "-246px", maxHeight: "384px" });
    vi.stubGlobal("innerWidth", 280);
    vi.stubGlobal("innerHeight", 280);
    fireEvent(window, new Event("resize"));
    expect(panel).toHaveStyle({ left: "3px", top: "-330px", maxHeight: "264px" });
    vi.unstubAllGlobals();
  });
});
