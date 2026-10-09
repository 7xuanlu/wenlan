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
      hidden={false}
      mode="labels"
      active="general"
      onSelect={() => {}}
      {...extraProps}
    />,
  );
}

describe("SettingsSidebar", () => {
  it("starts with Home and navigates to the wiki home", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onNavigateHome = vi.fn();
    renderSettingsSidebar({ onSelect, onNavigateHome });

    const home = screen.getByRole("button", { name: "Home" });
    const settingsLabel = screen.getByText("Settings");
    const general = screen.getByRole("button", { name: "General" });
    expect(home.compareDocumentPosition(settingsLabel) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(settingsLabel.compareDocumentPosition(general) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(home).not.toHaveAttribute("aria-expanded");
    expect(screen.queryByRole("heading", { name: "Wenlan" })).toBeNull();

    await user.click(home);

    expect(onNavigateHome).toHaveBeenCalledOnce();

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
    const { container } = renderSettingsSidebar({ hidden: true });
    const sidebar = container.querySelector(".settings-sidebar");

    expect(sidebar).toHaveAttribute("aria-hidden", "true");
    expect(sidebar).toHaveAttribute("inert");
    expect(sidebar?.querySelector("button")).not.toHaveFocus();
  });

  it("uses the compact Home entry without a visible label or shortcut", async () => {
    const user = userEvent.setup();
    const onNavigateHome = vi.fn();
    const { container } = renderSettingsSidebar({ mode: "icons", onNavigateHome });
    const home = screen.getByRole("button", { name: "Home" });

    expect(home).toHaveClass("notes-search-entry", "notes-search-entry--compact");
    expect(home).toHaveAttribute("title", "Home");
    expect(home).not.toHaveAttribute("aria-expanded");
    expect(container.querySelector(".notes-search-entry-label")).toBeNull();
    expect(container.querySelector(".notes-search-entry-shortcut")).toBeNull();

    await user.click(home);
    expect(onNavigateHome).toHaveBeenCalledOnce();
  });

  it("disables Home navigation when the caller disables navigation", async () => {
    const user = userEvent.setup();
    const onNavigateHome = vi.fn();
    renderSettingsSidebar({ navigationDisabled: true, onNavigateHome });
    const home = screen.getByRole("button", { name: "Home" });

    expect(home).toBeDisabled();
    await user.click(home);
    expect(onNavigateHome).not.toHaveBeenCalled();
  });

  it("focuses, traps, and closes the narrow overlay", async () => {
    const user = userEvent.setup();
    const onRequestClose = vi.fn();
    const onSelect = vi.fn();
    const onNavigateHome = vi.fn();
    const { container, rerender } = renderSettingsSidebar({
      hidden: true,
      active: "general",
      onSelect,
      open: false,
      presentation: "overlay",
      onRequestClose,
      onNavigateHome,
    });
    const sidebar = container.querySelector(".settings-sidebar");

    expect(sidebar).toHaveAttribute("aria-hidden", "true");
    expect(sidebar).toHaveAttribute("inert");

    rerender(
      <SettingsSidebar
        hidden={false}
        mode="labels"
        active="general"
        onSelect={onSelect}
        open
        presentation="overlay"
        onRequestClose={onRequestClose}
        onNavigateHome={onNavigateHome}
      />,
    );
    const first = screen.getByRole("button", { name: "Home" });
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
