// SPDX-License-Identifier: AGPL-3.0-only
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import AtlasSelect, { type AtlasSelectOption } from "./AtlasSelect";

const options: AtlasSelectOption[] = [
  { value: "", label: "All spaces" },
  { value: "creative", label: "Creative production" },
  { value: "engineering", label: "Engineering" },
];

function renderSelect(onChange = vi.fn(), value = "creative") {
  return {
    onChange,
    user: userEvent.setup(),
    ...render(
      <AtlasSelect
        label="Space"
        value={value}
        onChange={onChange}
        options={options}
        searchLabel="Search spaces"
        noMatchesLabel="No spaces found"
      />,
    ),
  };
}

describe("AtlasSelect", () => {
  it("opens with the current selection, filters options, and chooses by keyboard", async () => {
    const { user, onChange } = renderSelect();
    const trigger = screen.getByRole("combobox", { name: "Space" });

    await user.click(trigger);
    expect(screen.getByRole("option", { name: "Creative production" })).toHaveAttribute("aria-selected", "true");

    await user.type(screen.getByRole("combobox", { name: "Search spaces" }), "engi");
    expect(screen.getByRole("option", { name: "Engineering" })).toBeVisible();
    expect(screen.queryByRole("option", { name: "Creative production" })).not.toBeInTheDocument();
    await user.keyboard("{ArrowDown}{Enter}");

    expect(onChange).toHaveBeenCalledWith("engineering");
    expect(trigger).toHaveFocus();
  });

  it("supports Home and End navigation and marks the selected option", async () => {
    const { user } = renderSelect(vi.fn(), "engineering");
    await user.click(screen.getByRole("combobox", { name: "Space" }));
    const search = screen.getByRole("combobox", { name: "Search spaces" });
    await user.keyboard("{Home}");
    expect(screen.getByRole("combobox", { name: "Space" })).toHaveAttribute(
      "aria-activedescendant",
      expect.stringContaining("option-0"),
    );
    await user.keyboard("{End}");
    expect(screen.getByRole("combobox", { name: "Space" })).toHaveAttribute(
      "aria-activedescendant",
      expect.stringContaining("option-2"),
    );
    expect(search).toHaveFocus();
  });

  it("shows the empty state and dismisses on outside press or Escape", async () => {
    const { user } = renderSelect();
    const trigger = screen.getByRole("combobox", { name: "Space" });
    await user.click(trigger);
    await user.type(screen.getByRole("combobox", { name: "Search spaces" }), "missing");
    expect(screen.getByRole("status")).toHaveTextContent("No spaces found");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();

    await user.click(trigger);
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });
});
