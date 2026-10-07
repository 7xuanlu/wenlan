// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsSidebar from "./SettingsSidebar";

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => new Promise(() => {})),
}));

function renderSettingsSidebar(extraProps: Partial<React.ComponentProps<typeof SettingsSidebar>> = {}) {
  return render(
    <SettingsSidebar
      collapsed={false}
      active="general"
      onSelect={() => {}}
      {...extraProps}
    />,
  );
}

describe("SettingsSidebar", () => {
  it("starts with settings classification without a Home entry", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    renderSettingsSidebar({ onSelect });

    expect(screen.queryByRole("button", { name: "Home" })).toBeNull();
    expect(screen.queryByRole("heading", { name: "Wenlan" })).toBeNull();
    const general = screen.getByRole("button", { name: "General" });
    expect(screen.getByText("Settings").compareDocumentPosition(general) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    await user.click(general);

    expect(onSelect).toHaveBeenCalledWith("general");
  });

  it("keeps the Wenlan brand in the footer", () => {
    renderSettingsSidebar();

    const settingsLabel = screen.getByText("Settings");
    const brand = screen.getByText("Wenlan");
    expect(screen.queryByRole("button", { name: "Wenlan" })).toBeNull();

    expect(settingsLabel.compareDocumentPosition(brand) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("keeps collapsed desktop controls inert and out of the tab order", () => {
    const { container } = renderSettingsSidebar({ collapsed: true });
    const sidebar = container.querySelector(".settings-sidebar");

    expect(sidebar).toHaveAttribute("aria-hidden", "true");
    expect(sidebar).toHaveAttribute("inert");
    expect(sidebar?.querySelector("button")).not.toHaveFocus();
  });

  it("focuses, traps, and closes the narrow overlay", async () => {
    const user = userEvent.setup();
    const onRequestClose = vi.fn();
    const onSelect = vi.fn();
    const { container, rerender } = renderSettingsSidebar({
      collapsed: true,
      active: "general",
      onSelect,
      open: false,
      presentation: "overlay",
      onRequestClose,
    });
    const sidebar = container.querySelector(".settings-sidebar");

    expect(sidebar).toHaveAttribute("aria-hidden", "true");
    expect(sidebar).toHaveAttribute("inert");

    rerender(
      <SettingsSidebar
        collapsed={false}
        active="general"
        onSelect={onSelect}
        open
        presentation="overlay"
        onRequestClose={onRequestClose}
      />,
    );
    const first = screen.getByRole("button", { name: "General" });
    const last = screen.getByRole("button", { name: "Sources" });
    await waitFor(() => expect(first).toHaveFocus());

    last.focus();
    fireEvent.keyDown(last, { key: "Tab" });
    expect(first).toHaveFocus();
    fireEvent.keyDown(first, { key: "Tab", shiftKey: true });
    expect(last).toHaveFocus();

    await user.click(screen.getByRole("button", { name: "Close sidebar" }));
    expect(onRequestClose).toHaveBeenCalledOnce();
  });
});
