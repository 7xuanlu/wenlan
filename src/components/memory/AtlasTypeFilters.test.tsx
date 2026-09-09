// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import AtlasTypeFilters from "./AtlasTypeFilters";
import type { GraphPalette } from "../../lib/graph/palette";

const types: [string, number][] = [
  ["person", 8],
  ["technology", 5],
  ["theme", 2],
];

const palette: GraphPalette = {
  project: "#111111",
  tool: "#222222",
  org: "#333333",
  person: "#444444",
  concept: "#555555",
  neutral: "#666666",
  edge: "#777777",
  edgeStrong: "#888888",
  label: "#999999",
  labelMuted: "#aaaaaa",
  surface: "#bbbbbb",
  bridge: "#cccccc",
  graticule: "#dddddd",
  memory: "#eeeeee",
  page: "#ffffff",
};

function StatefulFilter({ initialExcluded = [] }: { initialExcluded?: string[] }) {
  const [excluded, setExcluded] = useState<Set<string>>(new Set(initialExcluded));
  return (
    <>
      <AtlasTypeFilters
        types={types}
        excluded={excluded}
        palette={palette}
        onToggle={(type) => setExcluded((current) => {
          const next = new Set(current);
          if (next.has(type)) next.delete(type);
          else next.add(type);
          return next;
        })}
        onReset={() => setExcluded(new Set())}
      />
      <button type="button" data-testid="outside">Outside</button>
    </>
  );
}

describe("AtlasTypeFilters", () => {
  it("opens a dialog with every type, including custom types, without check visuals", async () => {
    const user = userEvent.setup();
    render(<StatefulFilter />);

    const trigger = screen.getByRole("button", { name: "Filter entity types" });
    expect(trigger).toHaveTextContent("Filter entities");
    await user.click(trigger);

    const panel = screen.getByRole("dialog", { name: "Entity types" });
    expect(panel).toHaveTextContent("Showing 3 of 3 types");
    expect(within(panel).getByRole("button", { name: "Theme" })).toBeVisible();
    expect(panel.textContent).not.toMatch(/[✓✔]/);
    expect(within(panel).queryByRole("checkbox")).not.toBeInTheDocument();
    expect(panel).toHaveFocus();
  });

  it("toggles immediately while the panel stays open and reports filtered counts", async () => {
    const user = userEvent.setup();
    render(<StatefulFilter />);
    const trigger = screen.getByRole("button", { name: "Filter entity types" });
    await user.click(trigger);

    const panel = screen.getByRole("dialog", { name: "Entity types" });
    const person = within(panel).getByRole("button", { name: "Person" });
    await user.click(person);

    expect(screen.getByRole("dialog", { name: "Entity types" })).toBeInTheDocument();
    expect(person).toHaveAttribute("aria-pressed", "false");
    expect(panel).toHaveTextContent("Showing 2 of 3 types");
    expect(trigger).toHaveTextContent("Filter entities 2/3");
    expect(within(panel).getByRole("button", { name: "Restore all" })).not.toBeDisabled();
  });

  it("restores all types without closing the dialog", async () => {
    const user = userEvent.setup();
    render(<StatefulFilter initialExcluded={["person"]} />);
    const trigger = screen.getByRole("button", { name: "Filter entity types" });
    await user.click(trigger);
    const panel = screen.getByRole("dialog", { name: "Entity types" });

    expect(panel).toHaveTextContent("Showing 2 of 3 types");
    await user.click(within(panel).getByRole("button", { name: "Restore all" }));

    expect(screen.getByRole("dialog", { name: "Entity types" })).toBeInTheDocument();
    expect(panel).toHaveTextContent("Showing 3 of 3 types");
    expect(trigger).toHaveTextContent("Filter entities");
    expect(within(panel).getByRole("button", { name: "Restore all" })).toBeDisabled();
    expect(within(panel).getByRole("button", { name: "Person" })).toHaveAttribute("aria-pressed", "true");
    expect(panel).toHaveFocus();
  });

  it("dismisses on outside pointer or focus and restores focus on Escape", async () => {
    const user = userEvent.setup();
    render(<StatefulFilter />);
    const trigger = screen.getByRole("button", { name: "Filter entity types" });
    const outside = screen.getByTestId("outside");

    await user.click(trigger);
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("dialog", { name: "Entity types" })).not.toBeInTheDocument();

    await user.click(trigger);
    fireEvent.focus(outside);
    expect(screen.queryByRole("dialog", { name: "Entity types" })).not.toBeInTheDocument();

    const graphEscape = vi.fn();
    document.addEventListener("keydown", graphEscape);
    await user.click(trigger);
    await user.keyboard("{Escape}");
    document.removeEventListener("keydown", graphEscape);

    expect(graphEscape).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog", { name: "Entity types" })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
});
