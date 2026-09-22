import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../i18n";
import { RemoteAccessPanel } from "./RemoteAccessPanel";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
const mocks = vi.hoisted(() => Object.fromEntries([
  "toggleRemoteAccess", "getRemoteAccessStatus", "getRemoteAccessProfile",
  "configureRemoteAccess", "listSpaces", "inspectRemotePairing", "approveRemotePairing",
  "listRemoteGrants", "revokeRemoteGrant", "testRemoteMcpConnection", "clipboardWrite",
].map((key) => [key, vi.fn()])));
vi.mock("../../lib/tauri", () => mocks);

const profile = { revision: "r1", space: "review", enabled: true, disconnect_pending: false, credential_expires_at: Date.now() + 60_000 };
const pairing = { pairingId: "a".repeat(64), clientId: "synthetic-client", resource: "https://relay.wenlan.app/mcp", scopes: ["wenlan:query"], expiresAt: Date.now() + 60_000 };
const connected = { status: "connected", tunnel_url: null, relay_url: pairing.resource };
function panel(currentSpace?: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}><RemoteAccessPanel currentSpace={currentSpace} /></QueryClientProvider>);
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
  mocks.inspectRemotePairing.mockResolvedValue(pairing);
  mocks.approveRemotePairing.mockResolvedValue(undefined);
  mocks.listRemoteGrants.mockResolvedValue({ items: [], cursor: null });
  mocks.revokeRemoteGrant.mockResolvedValue({ revoked: true, cleanupPending: false });
  mocks.testRemoteMcpConnection.mockResolvedValue({ ok: true, latency_ms: 42, error: null });
  mocks.clipboardWrite.mockResolvedValue(undefined);
});
afterEach(async () => { await i18n.changeLanguage("en"); });

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
    mocks.getRemoteAccessProfile.mockRejectedValue(new Error("Storage unavailable"));
    panel();
    expect(await screen.findByRole("alert")).toHaveTextContent("Storage unavailable");
    expect(screen.getByRole("button", { name: "Web access" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Stop access" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(false));
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
  it("reconnect never starts if disconnect fails", async () => {
    await connectedPanel();
    mocks.toggleRemoteAccess.mockRejectedValue(new Error("Revocation unavailable"));
    fireEvent.click(screen.getByRole("button", { name: "Reconnect" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Revocation unavailable");
    expect(mocks.toggleRemoteAccess.mock.calls).toEqual([[false]]);
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
