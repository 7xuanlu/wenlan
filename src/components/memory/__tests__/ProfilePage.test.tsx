import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import ProfilePage from "../ProfilePage";

const listMemoriesRichMock = vi.hoisted(() =>
  vi.fn().mockImplementation((_domain, type) => {
    if (type === "preference") {
      return Promise.resolve([
        { source_id: "p1", title: "Strict TDD", content: "Always TDD", memory_type: "preference", confirmed: true, pinned: false, last_modified: 1744041600 },
      ]);
    }
    if (type === "identity") {
      return Promise.resolve([
        { source_id: "i1", title: "Ship wiki", content: "Launch this weekend", memory_type: "identity", confirmed: true, pinned: false, last_modified: 1744041600 },
      ]);
    }
    return Promise.resolve([]);
  }),
);

vi.mock("../../../lib/tauri", () => ({
  getProfile: vi.fn().mockResolvedValue({
    id: "1", name: "Lucian", display_name: "Lucian",
    email: "lucian@origin.dev", bio: "Building the knowledge layer",
    avatar_path: null, created_at: 1709251200,
  }),
  updateProfile: vi.fn().mockResolvedValue(null),
  setAvatar: vi.fn().mockResolvedValue(null),
  removeAvatar: vi.fn().mockResolvedValue(null),
  getProfileNarrative: vi.fn().mockResolvedValue({
    content: "You're a solo founder building Origin.",
    generated_at: 1744041600, is_stale: false, memory_count: 5,
  }),
  regenerateNarrative: vi.fn().mockResolvedValue(null),
  listMemoriesRich: listMemoriesRichMock,
  MEMORY_FACETS: [],
  FACET_COLORS: {},
}));

function wrap(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{ui}</QueryClientProvider>;
}

describe("ProfilePage (narrative-first)", () => {
  it("renders narrative brief", async () => {
    render(wrap(<ProfilePage onBack={() => {}} />));
    expect(await screen.findByText(/solo founder/)).toBeInTheDocument();
  });

  it("does not render Decision or Fact sections", async () => {
    render(wrap(<ProfilePage onBack={() => {}} />));
    await screen.findByText(/solo founder/);
    expect(screen.queryByText("Decision")).not.toBeInTheDocument();
    expect(screen.queryByText("Fact")).not.toBeInTheDocument();
  });

  it("does not query or render first-class goal memories", async () => {
    render(wrap(<ProfilePage onBack={() => {}} />));
    await screen.findByText(/solo founder/);
    expect(listMemoriesRichMock).not.toHaveBeenCalledWith(
      undefined,
      "goal",
      undefined,
      10,
    );
    expect(screen.queryByText(/Current focus/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/No active goals/i)).not.toBeInTheDocument();
  });

  it("shows preference pills", async () => {
    render(wrap(<ProfilePage onBack={() => {}} />));
    expect(await screen.findByText(/Strict TDD/)).toBeInTheDocument();
  });
});
