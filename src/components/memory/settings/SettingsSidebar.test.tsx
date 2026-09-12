// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsSidebar from "./SettingsSidebar";

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => new Promise(() => {})),
}));

// The sidebar carries the activity status line, which reads through
// react-query. Left unresolved the line renders nothing, which is the right
// behaviour for a read that has not landed and keeps these cases about the
// navigation they were written for.
vi.mock("../../../lib/tauri", () => ({
  getActivity: vi.fn(() => new Promise(() => {})),
}));

function renderSettingsSidebar(extraProps: Partial<React.ComponentProps<typeof SettingsSidebar>> = {}) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <SettingsSidebar
        collapsed={false}
        active="general"
        onSelect={() => {}}
        onNavigateHome={() => {}}
        {...extraProps}
      />
    </QueryClientProvider>,
  );
}

describe("SettingsSidebar", () => {
  it("uses Home as the top return affordance instead of a Wenlan heading", async () => {
    const user = userEvent.setup();
    const onNavigateHome = vi.fn();
    renderSettingsSidebar({ onNavigateHome });

    expect(screen.queryByRole("heading", { name: "Wenlan" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Home" }));

    expect(onNavigateHome).toHaveBeenCalledTimes(1);
  });

  it("keeps the Wenlan brand in the footer", () => {
    renderSettingsSidebar();

    const settingsLabel = screen.getByText("Settings");
    const brand = screen.getByRole("button", { name: "Wenlan" });

    expect(settingsLabel.compareDocumentPosition(brand) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
