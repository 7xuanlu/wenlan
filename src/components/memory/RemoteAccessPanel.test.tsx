import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { RemoteAccessPanel } from "./RemoteAccessPanel";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("../../lib/tauri", () => ({
  toggleRemoteAccess: vi.fn().mockResolvedValue({ status: "starting" }),
  getRemoteAccessStatus: vi.fn().mockResolvedValue({ status: "off" }),
  testRemoteMcpConnection: vi.fn().mockResolvedValue({ ok: true, latency_ms: 42, error: null }),
  clipboardWrite: vi.fn().mockResolvedValue(undefined),
}));

import {
  toggleRemoteAccess,
  getRemoteAccessStatus,
  testRemoteMcpConnection,
  clipboardWrite,
} from "../../lib/tauri";

function renderPanel(mode: "compact" | "full") {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  }
  return render(<RemoteAccessPanel mode={mode} />, { wrapper: Wrapper });
}

describe("RemoteAccessPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    (getRemoteAccessStatus as ReturnType<typeof vi.fn>).mockResolvedValue({ status: "off" });
    (toggleRemoteAccess as ReturnType<typeof vi.fn>).mockResolvedValue({ status: "starting" });
    (testRemoteMcpConnection as ReturnType<typeof vi.fn>).mockResolvedValue({
      ok: true,
      latency_ms: 42,
      error: null,
    });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders 'Off' status when disabled", async () => {
    renderPanel("compact");
    await waitFor(() => {
      expect(screen.getByText("Off")).toBeInTheDocument();
    });
  });

  it("states the no-auth URL boundary before Remote Access is enabled", async () => {
    renderPanel("compact");
    await waitFor(() => {
      expect(screen.getByText("Off")).toBeInTheDocument();
    });

    expect(screen.queryByText(/secure tunnel/i)).not.toBeInTheDocument();
    expect(screen.getByText(/no authentication/i)).toBeInTheDocument();
    expect(screen.getByText(/anyone with the URL can access Wenlan/i)).toBeInTheDocument();
    expect(screen.getByText(/turn Remote Access off when unused/i)).toBeInTheDocument();
  });

  it("renders 'Connecting…' when starting", async () => {
    (getRemoteAccessStatus as ReturnType<typeof vi.fn>).mockResolvedValue({ status: "starting" });
    renderPanel("compact");
    await waitFor(() => {
      expect(screen.getByText(/Connecting/i)).toBeInTheDocument();
    });
  });

  it("renders 'Connected' and URL when connected", async () => {
    (getRemoteAccessStatus as ReturnType<typeof vi.fn>).mockResolvedValue({
      status: "connected",
      tunnel_url: "https://example.trycloudflare.com",
      token: "secret-token",
      relay_url: "https://relay.origin.dev/abcdef/mcp",
    });
    renderPanel("compact");
    await waitFor(() => {
      expect(screen.getByText("Connected")).toBeInTheDocument();
    });
    expect(screen.getByText("https://relay.origin.dev/abcdef/mcp")).toBeInTheDocument();
  });

  it("clicking toggle calls toggleRemoteAccess", async () => {
    renderPanel("compact");
    await waitFor(() => {
      expect(screen.getByText("Off")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("switch"));
    await waitFor(() => {
      expect(toggleRemoteAccess).toHaveBeenCalledWith(true);
    });
  });

  it("Copy URL button shows 'Copied!' briefly", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    (getRemoteAccessStatus as ReturnType<typeof vi.fn>).mockResolvedValue({
      status: "connected",
      tunnel_url: "https://example.trycloudflare.com",
      token: "secret-token",
      relay_url: null,
    });
    renderPanel("compact");
    await waitFor(() => {
      expect(screen.getByText("Connected")).toBeInTheDocument();
    });
    const copyBtn = screen.getByRole("button", { name: /^Copy URL$/i });
    fireEvent.click(copyBtn);
    await waitFor(() => {
      expect(clipboardWrite).toHaveBeenCalledWith("https://example.trycloudflare.com/mcp");
    });
    expect(screen.getByText(/Copied!/i)).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(2100);
    });
    await waitFor(() => {
      expect(screen.queryByText(/Copied!/i)).not.toBeInTheDocument();
    });
  });

  it("Test connection button calls testRemoteMcpConnection and shows 'Connected (NNNms)'", async () => {
    (getRemoteAccessStatus as ReturnType<typeof vi.fn>).mockResolvedValue({
      status: "connected",
      tunnel_url: "https://example.trycloudflare.com",
      token: "secret-token",
      relay_url: null,
    });
    renderPanel("compact");
    await waitFor(() => {
      expect(screen.getByText("Connected")).toBeInTheDocument();
    });
    const testBtn = screen.getByRole("button", { name: /Test connection/i });
    fireEvent.click(testBtn);
    await waitFor(() => {
      expect(testRemoteMcpConnection).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect(screen.getByText(/Connected \(42ms\)/i)).toBeInTheDocument();
    });
  });

  it("Test connection failure shows error message", async () => {
    (testRemoteMcpConnection as ReturnType<typeof vi.fn>).mockResolvedValue({
      ok: false,
      latency_ms: null,
      error: "timeout after 5s",
    });
    (getRemoteAccessStatus as ReturnType<typeof vi.fn>).mockResolvedValue({
      status: "connected",
      tunnel_url: "https://example.trycloudflare.com",
      token: "secret-token",
      relay_url: null,
    });
    renderPanel("compact");
    await waitFor(() => {
      expect(screen.getByText("Connected")).toBeInTheDocument();
    });
    fireEvent.click(screen.getByRole("button", { name: /Test connection/i }));
    await waitFor(() => {
      expect(screen.getByText(/timeout after 5s/i)).toBeInTheDocument();
    });
  });

  it("compact mode does NOT render Token section", async () => {
    (getRemoteAccessStatus as ReturnType<typeof vi.fn>).mockResolvedValue({
      status: "connected",
      tunnel_url: "https://example.trycloudflare.com",
      token: "secret-token",
      relay_url: null,
    });
    renderPanel("compact");
    await waitFor(() => {
      expect(screen.getByText("Connected")).toBeInTheDocument();
    });
    expect(screen.queryByText(/Token/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Rotate$/i })).not.toBeInTheDocument();
  });

  it("full mode does not imply that the no-auth URL is token protected", async () => {
    (getRemoteAccessStatus as ReturnType<typeof vi.fn>).mockResolvedValue({
      status: "connected",
      tunnel_url: "https://example.trycloudflare.com",
      token: "secret-token",
      relay_url: null,
    });
    renderPanel("full");
    await waitFor(() => {
      expect(screen.getByText("Connected")).toBeInTheDocument();
    });
    expect(screen.queryByText(/^Token$/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Rotate$/i })).not.toBeInTheDocument();
  });
});
