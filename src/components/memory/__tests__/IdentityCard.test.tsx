// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../../i18n";
import IdentityCard from "../IdentityCard";

vi.mock("../../../lib/tauri", () => ({
  getProfile: vi.fn(),
  listEntities: vi.fn(),
  getEntityDetail: vi.fn(),
}));

import * as tauri from "../../../lib/tauri";

function renderIdentityCard() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onOpenSettings = vi.fn();
  const onOpenAbout = vi.fn();
  return render(
    <QueryClientProvider client={qc}>
      <IdentityCard
        onOpenDetail={() => {}}
        onOpenSettings={onOpenSettings}
        onOpenAbout={onOpenAbout}
      />
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  vi.clearAllMocks();
  await i18n.changeLanguage("en");
  vi.mocked(tauri.getProfile).mockResolvedValue({
    id: "p1",
    name: "Lucian",
    display_name: "Lucian",
    email: null,
    bio: null,
    avatar_path: "/missing/avatar.png",
    created_at: 0,
    updated_at: 0,
  } as any);
  vi.mocked(tauri.listEntities).mockResolvedValue([
    { id: "person-lucian", name: "Lucian", entity_type: "person" },
  ] as any);
  vi.mocked(tauri.getEntityDetail).mockResolvedValue({ observations: [] } as any);
});

describe("IdentityCard", () => {
  it("falls back to initials when the saved avatar cannot be loaded", async () => {
    renderIdentityCard();

    const avatar = await screen.findByRole("img", { name: "Lucian" });
    fireEvent.error(avatar);

    expect(screen.queryByRole("img", { name: "Lucian" })).not.toBeInTheDocument();
    expect(screen.getByText("L")).toBeInTheDocument();
  });

  it("renders an icon account entry with the name available on demand", async () => {
    vi.mocked(tauri.getEntityDetail).mockResolvedValue({
      observations: [{ content: "The user is a senior engineer working on Wenlan." }],
    } as any);

    renderIdentityCard();

    const trigger = await screen.findByRole("button", { name: /Lucian account menu/ });
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(tauri.getEntityDetail).not.toHaveBeenCalled();
    expect(trigger).toHaveAttribute("aria-haspopup", "menu");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveAttribute("title", "Lucian");
    expect(screen.queryByText("Lucian")).not.toBeInTheDocument();
    expect(screen.queryByText(/senior engineer/i)).not.toBeInTheDocument();
    expect(screen.queryByText("Set up your profile")).not.toBeInTheDocument();
  });

  it("opens a minimal avatar menu without profile-specific or tool-connection shortcuts", async () => {
    const user = userEvent.setup();
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const onOpenSettings = vi.fn();
    const onOpenAbout = vi.fn();

    render(
      <QueryClientProvider client={qc}>
        <IdentityCard
          onOpenDetail={() => {}}
          onOpenSettings={onOpenSettings}
          onOpenAbout={onOpenAbout}
        />
      </QueryClientProvider>,
    );

    await user.click(await screen.findByRole("button", { name: /Lucian account menu/ }));

    expect(screen.getByRole("menu")).toHaveClass("identity-rail-menu");
    expect(screen.queryByText("Lucian")).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "About Wenlan" })).toBeInTheDocument();
    expect(screen.queryByText("Profile settings")).not.toBeInTheDocument();
    expect(screen.queryByText("Connect tools")).not.toBeInTheDocument();
    expect(screen.queryByText("Local account")).not.toBeInTheDocument();

    await user.click(screen.getByRole("menuitem", { name: "Settings" }));
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
    expect(onOpenAbout).not.toHaveBeenCalled();
  });

  it("supports keyboard menu navigation and returns focus on Escape", async () => {
    const user = userEvent.setup();
    renderIdentityCard();
    const trigger = await screen.findByRole("button", { name: /Lucian account menu/ });
    await user.click(trigger);
    expect(screen.getByRole("menuitem", { name: "Settings" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "About Wenlan" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("does not mistake the first person in the knowledge base for the user", async () => {
    vi.mocked(tauri.getProfile).mockResolvedValue(null);
    renderIdentityCard();
    expect(await screen.findByRole("button", { name: "Account menu" })).toBeInTheDocument();
    expect(screen.queryByText("Lucian")).not.toBeInTheDocument();
    expect(tauri.listEntities).not.toHaveBeenCalled();
  });

  it("localizes the empty account menu state", async () => {
    await i18n.changeLanguage("zh-Hant");
    vi.mocked(tauri.getProfile).mockResolvedValue(null);
    vi.mocked(tauri.listEntities).mockResolvedValue([]);

    renderIdentityCard();

    expect(await screen.findByRole("button", { name: "帳戶選單" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "帳戶選單" })).toHaveAttribute("title", "帳戶");
    expect(screen.queryByText("設定你的個人資料")).not.toBeInTheDocument();
    expect(screen.queryByText("Set up your profile")).not.toBeInTheDocument();
  });
});
