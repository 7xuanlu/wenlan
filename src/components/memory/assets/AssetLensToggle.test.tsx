// SPDX-License-Identifier: AGPL-3.0-only
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { AssetLens } from "../../../lib/assetLens";
import { AssetLensToggle } from "./AssetLensToggle";

function renderToggle(value: AssetLens, onChange: (lens: AssetLens) => void = vi.fn()) {
  return { onChange, ...render(<AssetLensToggle onChange={onChange} value={value} />) };
}

describe("AssetLensToggle", () => {
  it("announces the group label for screen readers", () => {
    renderToggle("cards");
    expect(screen.getByRole("group", { name: "View" })).toBeInTheDocument();
  });

  it("reflects the active lens through aria-pressed", () => {
    const { unmount } = renderToggle("cards");
    expect(screen.getByRole("button", { name: "Cards" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Rows" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    unmount();

    renderToggle("rows");
    expect(screen.getByRole("button", { name: "Rows" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "Cards" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("reports the chosen lens without text labels", async () => {
    const user = userEvent.setup();
    const { onChange } = renderToggle("cards");

    // Icon-only: the accessible names exist, but no visible text does.
    expect(screen.queryByText("Rows")).toBeNull();
    expect(screen.queryByText("Cards")).toBeNull();
    expect(
      screen.getByRole("button", { name: "Rows" }),
    ).toHaveAccessibleName("Rows");

    await user.click(screen.getByRole("button", { name: "Rows" }));
    expect(onChange).toHaveBeenCalledWith("rows");
  });

  it("keeps both buttons keyboard-focusable", async () => {
    const user = userEvent.setup();
    renderToggle("rows");

    await user.tab();
    expect(screen.getByRole("button", { name: "Rows" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Cards" })).toHaveFocus();
  });
});
