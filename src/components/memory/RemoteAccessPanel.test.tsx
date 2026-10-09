import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../i18n";
import { RemoteAccessPanel } from "./RemoteAccessPanel";
import { clearPendingPairingCode, setPendingPairingCode } from "../../lib/pairingLink";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
const mocks = vi.hoisted(() => Object.fromEntries([
  "toggleRemoteAccess", "reconnectRemoteAccess", "getRemoteAccessStatus", "getRemoteAccessProfile",
  "configureRemoteAccess", "listSpaces", "inspectRemotePairing", "approveRemotePairing",
  "listRemoteGrants", "revokeRemoteGrant", "testRemoteMcpConnection", "clipboardWrite",
].map((key) => [key, vi.fn()])));
vi.mock("../../lib/tauri", () => mocks);

const profile = { revision: "r1", space: "review", enabled: true, disconnect_pending: false, credential_expires_at: Date.now() + 60_000 };
const pairing = { pairingId: "a".repeat(64), clientId: "synthetic-client", resource: "https://relay.wenlan.app/mcp", scopes: ["wenlan:query"], expiresAt: Date.now() + 60_000 };
const connected = { status: "connected", tunnel_url: null, relay_url: pairing.resource };
function panel(currentSpace?: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return { ...render(<QueryClientProvider client={client}><RemoteAccessPanel currentSpace={currentSpace} /></QueryClientProvider>), client };
}
async function connectedPanel() {
  mocks.getRemoteAccessProfile.mockResolvedValue(profile);
  mocks.getRemoteAccessStatus.mockResolvedValue(connected);
  panel();
  await screen.findByText("Device connected");
}
beforeEach(() => {
  vi.resetAllMocks();
  mocks.getRemoteAccessStatus.mockResolvedValue({ status: "off" });
  mocks.getRemoteAccessProfile.mockResolvedValue(null);
  mocks.listSpaces.mockResolvedValue([{ id: "s1", name: "review" }]);
  mocks.configureRemoteAccess.mockResolvedValue({ ...profile, enabled: false, revision: "configured" });
  mocks.toggleRemoteAccess.mockResolvedValue({ status: "starting" });
  mocks.reconnectRemoteAccess.mockResolvedValue({ status: "starting" });
  mocks.inspectRemotePairing.mockResolvedValue(pairing);
  mocks.approveRemotePairing.mockResolvedValue(undefined);
  mocks.listRemoteGrants.mockResolvedValue({ items: [], cursor: null });
  mocks.revokeRemoteGrant.mockResolvedValue({ revoked: true, cleanupPending: false });
  mocks.testRemoteMcpConnection.mockResolvedValue({ ok: true, latency_ms: 42, error: null });
  mocks.clipboardWrite.mockResolvedValue(undefined);
});
afterEach(async () => { cleanup(); vi.useRealTimers(); clearPendingPairingCode(); await i18n.changeLanguage("en"); });

describe("RemoteAccessPanel consent and connection", () => {
  it("requires explicit scope consent, without creating a single-Space selector", async () => {
    panel();
    const consent = await screen.findByRole("checkbox", { name: /Allow remote queries only in review/ });
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Web access" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Web access" })).toHaveAttribute("aria-pressed", "false");
    expect(mocks.toggleRemoteAccess).not.toHaveBeenCalled();
    fireEvent.click(consent);
    fireEvent.click(screen.getByRole("button", { name: "Web access" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(true, "configured"));
    expect(mocks.configureRemoteAccess).toHaveBeenCalledWith("review", undefined);
    expect(screen.queryByText(/no authentication/i)).not.toBeInTheDocument();
    expect(screen.queryByText("Ready")).not.toBeInTheDocument();
  });
  it("preselects the current existing Space, but clears consent after a scope change", async () => {
    mocks.listSpaces.mockResolvedValue([{ id: "a", name: "review" }, { id: "b", name: "private" }]);
    panel("review");
    const select = await screen.findByRole("combobox");
    await waitFor(() => expect(select).toHaveValue("review"));
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.change(select, { target: { value: "private" } });
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Web access" })).toBeDisabled();
  });
  it("does not guess a scope in an ambiguous multi-Space library", async () => {
    mocks.listSpaces.mockResolvedValue([{ id: "a", name: "review" }, { id: "b", name: "private" }]);
    panel();
    await waitFor(() => expect(screen.getByRole("combobox")).toHaveValue(""));
    expect(screen.getByRole("checkbox", { name: "Choose a Space before allowing remote queries." })).toBeDisabled();
    expect(screen.queryByText(/only in Choose a Space/)).not.toBeInTheDocument();
  });
  it("unknown native settings do not appear as permission to enable", async () => {
    mocks.getRemoteAccessProfile.mockRejectedValueOnce(new Error("Storage unavailable"));
    panel();
    expect(await screen.findByRole("alert")).toHaveTextContent("Storage unavailable");
    expect(screen.getByRole("status", { name: "Web access" })).toHaveTextContent("Unavailable");
    expect(screen.queryByRole("button", { name: "Web access" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Stop access" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(false));
  });
  it("replaces the unknown status after the failed read recovers", async () => {
    mocks.getRemoteAccessProfile.mockRejectedValueOnce(new Error("Storage unavailable"));
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    const { client } = panel();
    expect(await screen.findByRole("alert")).toHaveTextContent("Storage unavailable");
    expect(screen.getByRole("status", { name: "Web access" })).toHaveTextContent("Unavailable");

    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    await client.invalidateQueries({ queryKey: ["remote-access-profile"] });
    await waitFor(() => expect(screen.getByRole("button", { name: "Web access" })).toHaveAttribute("aria-pressed", "true"));
    expect(screen.queryByRole("status", { name: "Web access" })).not.toBeInTheDocument();
  });
  it("Space lookup failure does not disable stopping an existing connection", async () => {
    mocks.listSpaces.mockRejectedValue(new Error("Daemon offline"));
    await connectedPanel();
    await screen.findByRole("button", { name: "Stop access" });
    const toggle = screen.getByRole("button", { name: "Web access" });
    expect(toggle).not.toBeDisabled();
    fireEvent.click(toggle);
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(false));
  });
  it("copies only the fixed OAuth endpoint, never a direct tunnel fallback", async () => {
    await connectedPanel();
    fireEvent.click(screen.getByRole("button", { name: "Copy URL" }));
    await waitFor(() => expect(mocks.clipboardWrite).toHaveBeenCalledWith(pairing.resource));
    expect(screen.queryByText(/private.trycloudflare/)).not.toBeInTheDocument();
  });
  it("labels the URL for web and mobile apps and sends local apps to Add a tool", async () => {
    await connectedPanel();
    expect(screen.getByRole("heading", { name: "MCP URL for web and mobile apps" })).toBeInTheDocument();
    expect(screen.queryByText(/Codex MCP URL/)).not.toBeInTheDocument();
    expect(screen.getByText(/like Codex or Claude Code, use Add a tool above/)).toBeInTheDocument();
  });
  it("reconnect restarts transport at the saved revision without disabling or reconfiguring", async () => {
    await connectedPanel();
    fireEvent.click(screen.getByRole("button", { name: "Reconnect" }));
    await waitFor(() => expect(mocks.reconnectRemoteAccess).toHaveBeenCalledWith("r1"));
    expect(mocks.toggleRemoteAccess).not.toHaveBeenCalled();
    expect(mocks.configureRemoteAccess).not.toHaveBeenCalled();
    expect(mocks.revokeRemoteGrant).not.toHaveBeenCalled();
  });
  it("reconnect failure never falls back to disabling or enabling access", async () => {
    await connectedPanel();
    mocks.reconnectRemoteAccess.mockRejectedValue(new Error("Transport cleanup unconfirmed"));
    fireEvent.click(screen.getByRole("button", { name: "Reconnect" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Transport cleanup unconfirmed");
    expect(mocks.toggleRemoteAccess).not.toHaveBeenCalled();
  });
  it("pending disconnect blocks reconnect while preserving explicit Stop", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, enabled: false, disconnect_pending: true });
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    panel();
    await screen.findByRole("button", { name: "Retry disconnect" });
    expect(screen.getByRole("button", { name: "Reconnect" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Reconnect" }));
    expect(mocks.reconnectRemoteAccess).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Retry disconnect" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(false));
  });
  it("surfaces startup disconnect failure and allows stopping despite saved enabled intent", async () => {
    const warning = "Local transport stop requested; remote access settings are not confirmed (Remote access credentials could not be stored safely); server revoke failed: Remote connection unavailable; retry later; disconnect must be retried before app restart";
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue({ status: "error", error: warning });
    panel();
    expect(await screen.findByRole("alert")).toHaveTextContent(warning);
    expect(screen.queryByText("Device connected")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Stop access" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess.mock.calls).toEqual([[false]]));
  });
  it("allows shutdown retry when remote revoke succeeded but local processes remain unconfirmed", async () => {
    const warning = "Local remote-access processes have not been confirmed stopped. New connections are blocked; retry Stop access.";
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, enabled: false });
    mocks.getRemoteAccessStatus.mockResolvedValue({ status: "error", error: warning });
    panel();
    expect(await screen.findByRole("alert")).toHaveTextContent(warning);
    expect(screen.queryByText("Device connected")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Stop access" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess.mock.calls).toEqual([[false]]));
  });
  it("does not expose a tunnel when the native relay URL is missing", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue({ ...connected, relay_url: null });
    panel();
    await screen.findByText("Device connected");
    expect(screen.queryByRole("button", { name: "Copy URL" })).not.toBeInTheDocument();
    expect(screen.queryByText(/private.trycloudflare/)).not.toBeInTheDocument();
  });
  it("pending revoke blocks enabling and provides an explicit retry", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, enabled: false, disconnect_pending: true });
    panel();
    const retry = await screen.findByRole("button", { name: "Retry disconnect" });
    expect(screen.getByRole("button", { name: "Web access" })).toBeDisabled();
    fireEvent.click(retry);
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(false));
  });
  it("checks the actual native backend and does not claim client OAuth success", async () => {
    await connectedPanel();
    fireEvent.click(screen.getByRole("button", { name: "Test connection" }));
    expect(await screen.findByText("Backend and relay verified (42 ms)")).toBeInTheDocument();
  });
});

describe("pairing and grants", () => {
  it("inspection is not approval and approval preserves exact inspected intent", async () => {
    await connectedPanel();
    fireEvent.change(screen.getByRole("textbox", { name: "Pairing code" }), { target: { value: pairing.pairingId } });
    fireEvent.click(screen.getByRole("button", { name: "Review request" }));
    expect(await screen.findByText(pairing.clientId)).toBeInTheDocument();
    expect(mocks.approveRemotePairing).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Approve connection" }));
    await waitFor(() => expect(mocks.approveRemotePairing).toHaveBeenCalledWith("r1", pairing));
    expect(await screen.findByText("Approved. Complete the connection in your AI client.")).toBeInTheDocument();
  });
  it("changing the pairing code discards the inspected approval", async () => {
    await connectedPanel();
    const input = screen.getByRole("textbox", { name: "Pairing code" });
    fireEvent.change(input, { target: { value: pairing.pairingId } });
    fireEvent.click(screen.getByRole("button", { name: "Review request" }));
    await screen.findByRole("button", { name: "Approve connection" });
    fireEvent.change(input, { target: { value: "b".repeat(64) } });
    expect(screen.queryByRole("button", { name: "Approve connection" })).not.toBeInTheDocument();
    expect(mocks.approveRemotePairing).not.toHaveBeenCalled();
  });
  it("rejects expired inspection locally instead of sending approval", async () => {
    mocks.inspectRemotePairing.mockResolvedValue({ ...pairing, expiresAt: 1 });
    await connectedPanel();
    fireEvent.change(screen.getByRole("textbox", { name: "Pairing code" }), { target: { value: pairing.pairingId } });
    fireEvent.click(screen.getByRole("button", { name: "Review request" }));
    fireEvent.click(await screen.findByRole("button", { name: "Approve connection" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("expired or changed");
    expect(mocks.approveRemotePairing).not.toHaveBeenCalled();
  });
  it("shows when an authorized connection stops working", async () => {
    const expiresAt = Date.now() + 29 * 24 * 60 * 60 * 1000;
    mocks.listRemoteGrants.mockResolvedValue({ items: [
      { id: "g1", clientId: "client-A", space: "review", status: "active", cleanupPending: false, expiresAt },
      { id: "g2", clientId: "client-B", space: "review", status: "inactive", cleanupPending: false, expiresAt },
    ], cursor: null });
    await connectedPanel();
    const date = new Date(expiresAt).toLocaleDateString("en");
    expect(await screen.findAllByText(`Ends on ${date} at the latest, or after 30 days without use. Then connect again from your AI app.`)).toHaveLength(1);
    cleanup();
    await i18n.changeLanguage("zh-Hant");
    panel();
    // The date follows the app language, not the OS locale.
    const zhDate = new Date(expiresAt).toLocaleDateString("zh-Hant");
    expect(await screen.findByText(`最晚於 ${zhDate} 結束；連續 30 天未使用也會結束。之後請在 AI 應用程式中重新連線。`)).toBeInTheDocument();
  });
  it("shows authoritative revocation separately from token cleanup", async () => {
    mocks.listRemoteGrants.mockResolvedValue({ items: [{ id: "g1", clientId: "client-A", space: "review", status: "active", cleanupPending: false }], cursor: null });
    mocks.revokeRemoteGrant.mockResolvedValue({ revoked: true, cleanupPending: true });
    await connectedPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Revoke access" }));
    await waitFor(() => expect(mocks.revokeRemoteGrant).toHaveBeenCalledWith("r1", "g1"));
    expect(await screen.findByText("Access revoked. Stored token cleanup is pending.")).toBeInTheDocument();
  });
  it.each([["zh-Hant", "僅允許遠端查詢 review"], ["zh-Hans", "仅允许远程查询 review"]])("renders %s consent without English fallback", async (locale, consent) => {
    await i18n.changeLanguage(locale);
    panel();
    expect(await screen.findByText("共享 Space")).toBeInTheDocument();
    expect(await screen.findByText(new RegExp(consent))).toBeInTheDocument();
  });
});

describe("pairing link (wenlan://pair)", () => {
  const fromLink = "This request came from a link. Approve it only if you just started this connection in your AI app yourself.";
  it("fills in the code and opens the review, but never approves on its own", async () => {
    setPendingPairingCode(pairing.pairingId);
    await connectedPanel();
    await waitFor(() => expect(mocks.inspectRemotePairing).toHaveBeenCalledWith("r1", pairing.pairingId));
    expect(screen.getByRole("textbox", { name: "Pairing code" })).toHaveValue(pairing.pairingId);
    expect(await screen.findByText(pairing.clientId)).toBeInTheDocument();
    expect(screen.getByRole("note")).toHaveTextContent(fromLink);
    expect(mocks.approveRemotePairing).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Approve connection" }));
    await waitFor(() => expect(mocks.approveRemotePairing).toHaveBeenCalledWith("r1", pairing));
    expect(screen.queryByRole("note")).not.toBeInTheDocument();
  });
  it("takes the link once, so reopening the panel does not review it again", async () => {
    setPendingPairingCode(pairing.pairingId);
    await connectedPanel();
    await waitFor(() => expect(mocks.inspectRemotePairing).toHaveBeenCalledTimes(1));
    cleanup();
    await connectedPanel();
    expect(screen.getByRole("textbox", { name: "Pairing code" })).toHaveValue("");
    expect(mocks.inspectRemotePairing).toHaveBeenCalledTimes(1);
  });
  it("waits for Web access to connect before reviewing a linked code", async () => {
    const { listen } = await import("@tauri-apps/api/event");
    setPendingPairingCode(pairing.pairingId);
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue({ status: "starting" });
    panel();
    expect(await screen.findByText("Got a pairing request from a link. It opens here once Web access is connected.")).toBeInTheDocument();
    expect(mocks.inspectRemotePairing).not.toHaveBeenCalled();
    const handler = (listen as unknown as { mock: { calls: Array<[string, (event: { payload: unknown }) => void]> } }).mock.calls[0][1];
    // The native side reports connected from now on, to the event and to re-reads.
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    await act(async () => { handler({ payload: connected }); });
    await waitFor(() => expect(mocks.inspectRemotePairing).toHaveBeenCalledWith("r1", pairing.pairingId));
    expect(await screen.findByText(pairing.clientId)).toBeInTheDocument();
    expect(mocks.approveRemotePairing).not.toHaveBeenCalled();
  });
  it("editing the code drops the from-link warning", async () => {
    setPendingPairingCode(pairing.pairingId);
    await connectedPanel();
    await screen.findByRole("note");
    fireEvent.change(screen.getByRole("textbox", { name: "Pairing code" }), { target: { value: "b".repeat(64) } });
    fireEvent.click(screen.getByRole("button", { name: "Review request" }));
    await screen.findByRole("button", { name: "Approve connection" });
    expect(screen.queryByRole("note")).not.toBeInTheDocument();
  });
  it("keeps the warning on a link that arrives while another approval is still saving", async () => {
    const linked = "c".repeat(64);
    let finishApprove: () => void = () => {};
    mocks.approveRemotePairing.mockImplementationOnce(() => new Promise<void>((resolve) => { finishApprove = resolve; }));
    await connectedPanel();
    fireEvent.change(screen.getByRole("textbox", { name: "Pairing code" }), { target: { value: pairing.pairingId } });
    fireEvent.click(screen.getByRole("button", { name: "Review request" }));
    fireEvent.click(await screen.findByRole("button", { name: "Approve connection" }));
    await waitFor(() => expect(mocks.approveRemotePairing).toHaveBeenCalledTimes(1));
    act(() => { setPendingPairingCode(linked); });
    await act(async () => { finishApprove(); });
    await waitFor(() => expect(mocks.inspectRemotePairing).toHaveBeenLastCalledWith("r1", linked));
    expect(await screen.findByRole("note")).toHaveTextContent(fromLink);
    expect(screen.getByRole("textbox", { name: "Pairing code" })).toHaveValue(linked);
  });
  it("says so when a link arrives while Web access is off", async () => {
    setPendingPairingCode(pairing.pairingId);
    panel();
    expect(await screen.findByText(/but Web access is off/)).toBeInTheDocument();
    expect(mocks.inspectRemotePairing).not.toHaveBeenCalled();
  });
});

describe("delayed grant reconciliation", () => {
  // Fake timers are installed BEFORE mounting so the grant refetchInterval is
  // owned by the fake clock; advancing it exercises the real polling path.
  async function settle(rounds = 8) {
    for (let i = 0; i < rounds; i += 1) {
      await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    }
  }
  it("shows an externally created grant after approval without manual refresh", async () => {
    vi.useFakeTimers();
    try {
      mocks.getRemoteAccessProfile.mockResolvedValue(profile);
      mocks.getRemoteAccessStatus.mockResolvedValue(connected);
      mocks.listRemoteGrants.mockResolvedValue({ items: [], cursor: null });
      panel();
      await settle();
      expect(screen.getByText("Device connected")).toBeInTheDocument();
      expect(screen.getByText("No authorized connections")).toBeInTheDocument();
      const callsBeforeApprove = mocks.listRemoteGrants.mock.calls.length;
      fireEvent.change(screen.getByRole("textbox", { name: "Pairing code" }), { target: { value: pairing.pairingId } });
      fireEvent.click(screen.getByRole("button", { name: "Review request" }));
      await settle();
      fireEvent.click(screen.getByRole("button", { name: "Approve connection" }));
      await settle();
      expect(mocks.approveRemotePairing).toHaveBeenCalledWith("r1", pairing);
      // Approval invalidation settles to the empty result BEFORE the external
      // OAuth exchange creates the grant.
      expect(mocks.listRemoteGrants.mock.calls.length).toBeGreaterThan(callsBeforeApprove);
      expect(screen.getByText("No authorized connections")).toBeInTheDocument();
      mocks.listRemoteGrants.mockResolvedValue({ items: [{ id: "g1", clientId: pairing.clientId, space: "review", status: "active", cleanupPending: false }], cursor: null });
      const callsBeforeInterval = mocks.listRemoteGrants.mock.calls.length;
      await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
      await settle();
      expect(mocks.listRemoteGrants.mock.calls.length).toBeGreaterThan(callsBeforeInterval);
      expect(screen.getByText(pairing.clientId)).toBeInTheDocument();
      expect(screen.getByText("Authorized")).toBeInTheDocument();
    } finally { vi.useRealTimers(); }
  });
  it("shows a revoked grant as inactive and calls revoke with the exact grant id", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    mocks.listRemoteGrants.mockResolvedValue({ items: [{ id: "g9", clientId: "client-Z", space: "review", status: "inactive", cleanupPending: false }], cursor: null });
    mocks.revokeRemoteGrant.mockResolvedValue({ revoked: true, cleanupPending: false });
    panel();
    await screen.findByText("Device connected");
    expect(await screen.findByText("Access revoked")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Revoke access" })).not.toBeInTheDocument();
    expect(mocks.revokeRemoteGrant).not.toHaveBeenCalled();
    // Active grant path still revokes with exact revision + id.
    cleanup();
    mocks.listRemoteGrants.mockResolvedValue({ items: [{ id: "g9", clientId: "client-Z", space: "review", status: "active", cleanupPending: false }], cursor: null });
    panel();
    await screen.findByText("Device connected");
    fireEvent.click(await screen.findByRole("button", { name: "Revoke access" }));
    await waitFor(() => expect(mocks.revokeRemoteGrant).toHaveBeenCalledWith("r1", "g9"));
    await waitFor(() => expect(mocks.revokeRemoteGrant).toHaveBeenCalledTimes(1));
  });
  it("stops polling after a grant error until manual refresh succeeds", async () => {
    vi.useFakeTimers();
    try {
      mocks.getRemoteAccessProfile.mockResolvedValue(profile);
      mocks.getRemoteAccessStatus.mockResolvedValue(connected);
      mocks.listRemoteGrants.mockRejectedValueOnce(new Error("Grants offline"));
      mocks.listRemoteGrants.mockResolvedValue({ items: [], cursor: null });
      panel();
      await settle();
      expect(screen.getByText("Device connected")).toBeInTheDocument();
      expect(screen.getByText(/Grants offline/)).toBeInTheDocument();
      const callsAfterError = mocks.listRemoteGrants.mock.calls.length;
      expect(callsAfterError).toBeGreaterThan(0);
      // Paused interval issues no background requests across three periods.
      await act(async () => { await vi.advanceTimersByTimeAsync(15000); });
      expect(mocks.listRemoteGrants.mock.calls.length).toBe(callsAfterError);
      fireEvent.click(screen.getByRole("button", { name: "Refresh connections" }));
      await settle();
      expect(mocks.listRemoteGrants.mock.calls.length).toBeGreaterThan(callsAfterError);
      expect(screen.getByText("No authorized connections")).toBeInTheDocument();
      // Recovery resumes the interval: the next period polls again.
      const callsAfterRecovery = mocks.listRemoteGrants.mock.calls.length;
      await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
      expect(mocks.listRemoteGrants.mock.calls.length).toBeGreaterThan(callsAfterRecovery);
    } finally { vi.useRealTimers(); }
  });
  it("refetches grants on remote-access-status events, and disconnect stops polling", async () => {
    vi.useFakeTimers();
    try {
      const { listen } = await import("@tauri-apps/api/event");
      mocks.getRemoteAccessProfile.mockResolvedValue(profile);
      mocks.getRemoteAccessStatus.mockResolvedValue(connected);
      mocks.listRemoteGrants.mockResolvedValue({ items: [], cursor: null });
      const view = panel();
      await settle();
      expect(screen.getByText("Device connected")).toBeInTheDocument();
      expect(mocks.listRemoteGrants).toHaveBeenCalled();
      const handler = (listen as unknown as { mock: { calls: Array<[string, (event: { payload: unknown }) => void]> } }).mock.calls[0][1];
      mocks.listRemoteGrants.mockResolvedValue({ items: [{ id: "g2", clientId: "client-event", space: "review", status: "active", cleanupPending: false }], cursor: null });
      await act(async () => { handler({ payload: connected }); });
      await settle();
      expect(screen.getByText("client-event")).toBeInTheDocument();
      // Disconnect: status leaves connected, so polling must stop entirely.
      await act(async () => { handler({ payload: { status: "off" } }); });
      await settle();
      const callsAfterDisconnect = mocks.listRemoteGrants.mock.calls.length;
      await act(async () => { await vi.advanceTimersByTimeAsync(15000); });
      expect(mocks.listRemoteGrants.mock.calls.length).toBe(callsAfterDisconnect);
      view.unmount();
      await act(async () => { await vi.advanceTimersByTimeAsync(15000); });
      expect(mocks.listRemoteGrants.mock.calls.length).toBe(callsAfterDisconnect);
    } finally { vi.useRealTimers(); }
  });
  it("stops an active polling interval when the panel unmounts", async () => {
    vi.useFakeTimers();
    try {
      mocks.getRemoteAccessProfile.mockResolvedValue(profile);
      mocks.getRemoteAccessStatus.mockResolvedValue(connected);
      const view = panel();
      await settle();
      const initialCalls = mocks.listRemoteGrants.mock.calls.length;
      expect(initialCalls).toBeGreaterThan(0);
      await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
      await settle();
      expect(mocks.listRemoteGrants.mock.calls.length).toBeGreaterThan(initialCalls);
      view.unmount();
      const callsAtUnmount = mocks.listRemoteGrants.mock.calls.length;
      await act(async () => { await vi.advanceTimersByTimeAsync(15000); });
      expect(mocks.listRemoteGrants.mock.calls.length).toBe(callsAtUnmount);
    } finally { vi.useRealTimers(); }
  });
});
