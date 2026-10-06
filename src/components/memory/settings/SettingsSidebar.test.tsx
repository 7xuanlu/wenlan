// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
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
});
