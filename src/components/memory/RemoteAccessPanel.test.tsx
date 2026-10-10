import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup, act, within, configure } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../i18n";
import { RemoteAccessPanel } from "./RemoteAccessPanel";
import {
  clearAwaitingConnection, clearPendingPairingCode, markPairingApproved, usePendingPairingCode,
} from "../../lib/pairingLink";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
const mocks = vi.hoisted(() => Object.fromEntries([
  "toggleRemoteAccess", "reconnectRemoteAccess", "renewRemoteAccess", "getRemoteAccessStatus",
  "getRemoteAccessProfile", "configureRemoteAccess", "listSpaces", "listRemoteGrants",
  "revokeRemoteGrant", "testRemoteMcpConnection", "clipboardWrite",
].map((key) => [key, vi.fn()])));
vi.mock("../../lib/tauri", () => mocks);

const DAY = 24 * 60 * 60 * 1000;
const profile = { revision: "r1", space: "review", enabled: true, disconnect_pending: false, credential_expires_at: Date.now() + 60 * DAY };
const relayUrl = "https://relay.wenlan.app/mcp";
const connected = { status: "connected", tunnel_url: null, relay_url: relayUrl };
const twoSpaces = [{ id: "a", name: "review" }, { id: "b", name: "private" }];
const grant = (over: Record<string, unknown> = {}) => ({
  id: "g1", clientId: "synthetic-client-id", space: "review", status: "active", cleanupPending: false,
  createdAt: Date.now() - DAY, expiresAt: Date.now() + 29 * DAY,
  clientName: "Claude", redirectHost: "claude.ai", knownClient: true, lastUsedAt: Date.now() - 5 * 60_000, endReason: null,
  ...over,
});

function CodeProbe() {
  const code = usePendingPairingCode();
  return <span data-testid="pending-code">{code ?? "none"}</span>;
}
function panel(currentSpace?: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return { ...render(<QueryClientProvider client={client}><RemoteAccessPanel currentSpace={currentSpace} /><CodeProbe /></QueryClientProvider>), client };
}
async function connectedPanel() {
  mocks.getRemoteAccessProfile.mockResolvedValue(profile);
  mocks.getRemoteAccessStatus.mockResolvedValue(connected);
  const view = panel();
  await screen.findByText("Connected");
  return view;
}
const button = (name: string | RegExp) => screen.getByRole("button", { name });

// jsdom accessible-name queries are slow on a busy machine; the first test of a
// file also pays the cold start. 4s still fails ahead of the 5s test timeout.
configure({ asyncUtilTimeout: 4_000 });

beforeEach(() => {
  vi.resetAllMocks();
  mocks.getRemoteAccessStatus.mockResolvedValue({ status: "off" });
  mocks.getRemoteAccessProfile.mockResolvedValue(null);
  mocks.listSpaces.mockResolvedValue([{ id: "s1", name: "review" }]);
  mocks.configureRemoteAccess.mockResolvedValue({ ...profile, enabled: false, revision: "configured" });
  mocks.toggleRemoteAccess.mockResolvedValue({ status: "starting" });
  mocks.reconnectRemoteAccess.mockResolvedValue({ status: "starting" });
  mocks.renewRemoteAccess.mockResolvedValue({ status: "starting" });
  mocks.listRemoteGrants.mockResolvedValue({ items: [], cursor: null });
  mocks.revokeRemoteGrant.mockResolvedValue({ revoked: true, cleanupPending: false });
  mocks.testRemoteMcpConnection.mockResolvedValue({ ok: true, latency_ms: 42, error: null });
  mocks.clipboardWrite.mockResolvedValue(undefined);
});
afterEach(async () => {
  cleanup(); vi.useRealTimers(); clearPendingPairingCode(); clearAwaitingConnection();
  await i18n.changeLanguage("en");
});

describe("turning on", () => {
  it("is one click: no consent checkbox, no choose-a-Space-first copy", async () => {
    panel();
    const turnOn = await screen.findByRole("button", { name: "Turn on" });
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(screen.queryByText(/choose a space/i)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Web access" })).not.toBeInTheDocument();
    expect(screen.getByText("Off")).toBeInTheDocument();
    await waitFor(() => expect(turnOn).toBeEnabled());
    expect(mocks.toggleRemoteAccess).not.toHaveBeenCalled();
    fireEvent.click(turnOn);
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(true, "configured"));
    expect(mocks.configureRemoteAccess).toHaveBeenCalledWith("review", undefined);
  });

  it("shows the Space it will share before turning on, starting from the one being viewed", async () => {
    mocks.listSpaces.mockResolvedValue(twoSpaces);
    panel("private");
    const select = await screen.findByRole("combobox", { name: "Space to share" });
    await waitFor(() => expect(select).toHaveValue("private"));
    fireEvent.change(select, { target: { value: "review" } });
    fireEvent.click(button("Turn on"));
    await waitFor(() => expect(mocks.configureRemoteAccess).toHaveBeenCalledWith("review", undefined));
  });

  it("starts from the first Space when nothing else says which", async () => {
    mocks.listSpaces.mockResolvedValue(twoSpaces);
    panel();
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Space to share" })).toHaveValue("review"));
    expect(button("Turn on")).toBeEnabled();
  });

  it("offers 'Turn on again' once Web access has been set up before", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, enabled: false });
    panel();
    expect(await screen.findByRole("button", { name: "Turn on again" })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Space to share" })).toHaveValue("review"));
  });

  it("needs a Space to share", async () => {
    mocks.listSpaces.mockResolvedValue([]);
    panel();
    expect(await screen.findByText("Create a Space first. Web access shares one Space.")).toBeInTheDocument();
    expect(button("Turn on")).toBeDisabled();
  });

  it("says Connecting while it starts", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue({ status: "starting" });
    panel();
    expect(await screen.findByText("Connecting…")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Test connection" })).not.toBeInTheDocument();
  });

  it("says why turning on failed, in plain words, with the raw sentence behind Details", async () => {
    mocks.toggleRemoteAccess.mockRejectedValue(new Error("Remote connection rate limited"));
    panel();
    await waitFor(() => expect(button("Turn on")).toBeEnabled());
    fireEvent.click(button("Turn on"));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Too many tries. Wait a few minutes, then try again.");
    expect(screen.queryByText(/rate limited/)).not.toBeInTheDocument();
    fireEvent.click(button("Details"));
    expect(screen.getByText("Remote connection rate limited")).toBeInTheDocument();
  });

  it("unknown native settings do not appear as permission to turn on, but Stop access stays", async () => {
    mocks.getRemoteAccessProfile.mockRejectedValueOnce(new Error("Storage unavailable"));
    panel();
    expect(await screen.findByRole("alert")).toHaveTextContent("Something went wrong. Try again.");
    expect(screen.queryByRole("button", { name: "Turn on" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Turn on again" })).not.toBeInTheDocument();
    fireEvent.click(button("Stop access"));
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(false));
  });

  it("replaces the failure after the read recovers", async () => {
    mocks.getRemoteAccessProfile.mockRejectedValueOnce(new Error("Storage unavailable"));
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    const { client } = panel();
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    await client.invalidateQueries({ queryKey: ["remote-access-profile"] });
    expect(await screen.findByText("Connected")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Stop access" })).not.toBeInTheDocument();
  });

  it("a failed Space lookup does not stop an existing connection from being turned off", async () => {
    mocks.listSpaces.mockRejectedValue(new Error("Daemon offline"));
    await connectedPanel();
    fireEvent.click(button("Turn off"));
    fireEvent.click(within(await screen.findByRole("group")).getByRole("button", { name: "Turn off" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(false));
  });
});

describe("a connection that is on", () => {
  it("shows status, the Space, and three plain steps with the URL to copy", async () => {
    await connectedPanel();
    expect(screen.getByText("Sharing review")).toBeInTheDocument();
    const steps = screen.getAllByRole("listitem").slice(0, 3);
    expect(steps[0]).toHaveTextContent("Copy this URL.");
    expect(steps[0]).toHaveTextContent(relayUrl);
    expect(steps[1]).toHaveTextContent("In Claude or ChatGPT, add it as a custom connector.");
    expect(steps[2]).toHaveTextContent("When Wenlan asks, click Allow.");
    expect(screen.getByText(/like Codex or Claude Code, use Add a tool above/)).toBeInTheDocument();
    expect(await screen.findByText("No apps connected yet")).toBeInTheDocument();
  });

  it("copies only the relay URL", async () => {
    await connectedPanel();
    fireEvent.click(button("Copy URL"));
    await waitFor(() => expect(mocks.clipboardWrite).toHaveBeenCalledWith(relayUrl));
  });

  it("has nothing to copy, and no steps, when the relay URL is missing", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue({ ...connected, relay_url: null });
    panel();
    await screen.findByText("Connected");
    expect(screen.queryByRole("button", { name: "Copy URL" })).not.toBeInTheDocument();
    expect(screen.queryByText("Copy this URL.")).not.toBeInTheDocument();
  });

  it("reconnect restarts at the saved revision without turning off or reconfiguring", async () => {
    await connectedPanel();
    fireEvent.click(button("Reconnect"));
    await waitFor(() => expect(mocks.reconnectRemoteAccess).toHaveBeenCalledWith("r1"));
    expect(mocks.toggleRemoteAccess).not.toHaveBeenCalled();
    expect(mocks.configureRemoteAccess).not.toHaveBeenCalled();
    expect(mocks.revokeRemoteGrant).not.toHaveBeenCalled();
  });

  it("a failed reconnect never falls back to turning anything off or on", async () => {
    await connectedPanel();
    mocks.reconnectRemoteAccess.mockRejectedValue(new Error("Transport cleanup unconfirmed"));
    fireEvent.click(button("Reconnect"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Something went wrong. Try again.");
    expect(mocks.toggleRemoteAccess).not.toHaveBeenCalled();
  });

  it("Test connection says what it proved, and nothing more", async () => {
    await connectedPanel();
    fireEvent.click(button("Test connection"));
    expect(await screen.findByText("Connected. Wenlan's web service sees this computer (42 ms).")).toBeInTheDocument();
    expect(screen.queryByText(/backend and relay verified/i)).not.toBeInTheDocument();
  });

  it("Test connection failure is plain, with the reason behind Details", async () => {
    mocks.testRemoteMcpConnection.mockResolvedValue({ ok: false, latency_ms: null, error: "relay does not see this device (HTTP 502)" });
    await connectedPanel();
    fireEvent.click(button("Test connection"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Connection check failed");
    expect(screen.queryByText(/502/)).not.toBeInTheDocument();
    fireEvent.click(button("Details"));
    expect(screen.getByText("relay does not see this device (HTTP 502)")).toBeInTheDocument();
  });

  it("shows a status error in plain words and offers Stop access", async () => {
    const warning = "Local transport stop requested; remote access settings are not confirmed; server revoke failed: Remote connection unavailable; retry later; disconnect must be retried before app restart";
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue({ status: "error", error: warning });
    panel();
    expect(await screen.findByRole("alert")).toHaveTextContent("Can't reach Wenlan's web service. Check your internet connection.");
    expect(screen.queryByText("Connected")).not.toBeInTheDocument();
    expect(screen.queryByText(/revoke failed/)).not.toBeInTheDocument();
    fireEvent.click(button("Stop access"));
    await waitFor(() => expect(mocks.toggleRemoteAccess.mock.calls).toEqual([[false]]));
  });

  it("allows another Stop when local processes are unconfirmed, and offers no Turn on beside it", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, enabled: false });
    mocks.getRemoteAccessStatus.mockResolvedValue({ status: "error", error: "Local remote-access processes have not been confirmed stopped. New connections are blocked; retry Stop access." });
    panel();
    await screen.findByRole("alert");
    expect(screen.queryByRole("button", { name: /^Turn on/ })).not.toBeInTheDocument();
    fireEvent.click(button("Stop access"));
    await waitFor(() => expect(mocks.toggleRemoteAccess.mock.calls).toEqual([[false]]));
  });

  it("a pending disconnect blocks turning on and reconnecting, and offers Retry disconnect", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, enabled: false, disconnect_pending: true });
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    panel();
    fireEvent.click(await screen.findByRole("button", { name: "Retry disconnect" }));
    expect(screen.queryByRole("button", { name: "Reconnect" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Turn on/ })).not.toBeInTheDocument();
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(false));
    expect(mocks.reconnectRemoteAccess).not.toHaveBeenCalled();
  });
});

describe("asking before it ends connections", () => {
  it("turn off asks first, Escape cancels, then it turns off", async () => {
    await connectedPanel();
    fireEvent.click(button("Turn off"));
    const ask = await screen.findByRole("group");
    expect(ask).toHaveTextContent("Turn off Web access? Connected web apps lose access right away.");
    fireEvent.keyDown(ask, { key: "Escape" });
    expect(screen.queryByRole("group")).not.toBeInTheDocument();
    expect(mocks.toggleRemoteAccess).not.toHaveBeenCalled();
    fireEvent.click(button("Turn off"));
    fireEvent.click(within(await screen.findByRole("group")).getByRole("button", { name: "Turn off" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess).toHaveBeenCalledWith(false));
  });

  it("changing the Space turns off, saves the new Space, and turns on again, after a confirm", async () => {
    mocks.listSpaces.mockResolvedValue(twoSpaces);
    await connectedPanel();
    fireEvent.click(button("Change Space"));
    const select = await screen.findByRole("combobox", { name: "Space to share" });
    expect(select).toHaveValue("review");
    expect(screen.queryByRole("group")).not.toBeInTheDocument();
    fireEvent.change(select, { target: { value: "private" } });
    const ask = await screen.findByRole("group");
    expect(ask).toHaveTextContent("Share private instead? Web apps connected now will need to connect again.");
    expect(mocks.toggleRemoteAccess).not.toHaveBeenCalled();
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, enabled: false, revision: "r5" });
    fireEvent.click(within(ask).getByRole("button", { name: "Change" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess.mock.calls).toEqual([[false], [true, "configured"]]));
    expect(mocks.configureRemoteAccess).toHaveBeenCalledWith("private", "r5");
  });

  it("cannot change the Space when there is no other Space", async () => {
    await connectedPanel();
    expect(button("Change Space")).toBeDisabled();
  });

  it("removing an app asks, then revokes at the exact revision and grant", async () => {
    mocks.listRemoteGrants.mockResolvedValue({ items: [grant({ id: "g9" })], cursor: null });
    await connectedPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Remove access" }));
    const ask = await screen.findByRole("group");
    expect(ask).toHaveTextContent("Remove access for Claude? It will need to connect again.");
    expect(mocks.revokeRemoteGrant).not.toHaveBeenCalled();
    fireEvent.click(within(ask).getByRole("button", { name: "Remove access" }));
    await waitFor(() => expect(mocks.revokeRemoteGrant).toHaveBeenCalledWith("r1", "g9"));
    expect(mocks.revokeRemoteGrant).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("Ended: access was removed")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove access" })).not.toBeInTheDocument();
  });

  it("shows revocation separately from token cleanup, and offers a retry", async () => {
    mocks.listRemoteGrants.mockResolvedValue({ items: [grant()], cursor: null });
    mocks.revokeRemoteGrant.mockResolvedValue({ revoked: true, cleanupPending: true });
    await connectedPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Remove access" }));
    fireEvent.click(within(await screen.findByRole("group")).getByRole("button", { name: "Remove access" }));
    expect(await screen.findByText("Access revoked. Stored token cleanup is pending.")).toBeInTheDocument();
    mocks.revokeRemoteGrant.mockResolvedValue({ revoked: true, cleanupPending: false });
    fireEvent.click(button("Retry"));
    await waitFor(() => expect(mocks.revokeRemoteGrant).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByText("Access revoked. Stored token cleanup is pending.")).not.toBeInTheDocument());
  });
});

describe("connected apps", () => {
  it.each([
    ["still connecting", { status: "starting" }],
    ["cannot reach the service", { status: "error", error: "Remote connection unavailable; retry later" }],
  ])("shows no empty list header while %s, and never asks the relay", async (_label, status) => {
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue(status);
    panel();
    await screen.findByRole("button", { name: "Reconnect" });
    expect(screen.queryByText("Connected apps")).not.toBeInTheDocument();
    expect(mocks.listRemoteGrants).not.toHaveBeenCalled();
  });

  it("reads in plain words: who, last used, when it ends, and why one ended", async () => {
    const endsAt = Date.now() + 20 * DAY;
    mocks.listRemoteGrants.mockResolvedValue({ items: [
      grant({ expiresAt: endsAt }),
      grant({ id: "g2", clientId: "client-two", clientName: "Mystery", redirectHost: "other.example", knownClient: false, lastUsedAt: null }),
      grant({ id: "g3", clientId: "client-three", clientName: null, redirectHost: null, knownClient: false }),
      grant({ id: "g4", clientId: "client-four", redirectHost: "chatgpt.com", status: "inactive", endReason: "reset" }),
      grant({ id: "g5", clientId: "client-five", status: "inactive", endReason: "expired" }),
      grant({ id: "g6", clientId: "client-six", status: "inactive", endReason: "replaced" }),
      grant({ id: "g7", clientId: "client-seven", status: "inactive", endReason: "other" }),
    ], cursor: null });
    await connectedPanel();
    const date = new Date(endsAt).toLocaleDateString("en");
    expect(await screen.findByText(`Last used 5m ago · Ends ${date}`)).toBeInTheDocument();
    expect(screen.getByText("other.example")).toBeInTheDocument();
    expect(screen.getByText(/Not used yet/)).toBeInTheDocument();
    expect(screen.getByText("Unrecognized app")).toBeInTheDocument();
    expect(screen.queryByText("Mystery")).not.toBeInTheDocument();
    expect(screen.getByText("ChatGPT")).toBeInTheDocument();
    expect(screen.getByText("Ended: Web access was renewed or its Space changed")).toBeInTheDocument();
    expect(screen.getByText("Ended: it expired")).toBeInTheDocument();
    expect(screen.getByText("Ended: a newer connection replaced it")).toBeInTheDocument();
    expect(screen.getByText("Ended")).toBeInTheDocument();
    // The client id is not in the list, only behind Details.
    expect(screen.queryByText("synthetic-client-id")).not.toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "Details" })[0]);
    expect(screen.getByText("synthetic-client-id")).toBeInTheDocument();
  });

  it("works against a relay that does not send the newer fields", async () => {
    mocks.listRemoteGrants.mockResolvedValue({ items: [
      { id: "old", clientId: "legacy-client", space: "review", status: "active", cleanupPending: false, createdAt: Date.now(), expiresAt: Date.now() + 29 * DAY },
      { id: "old2", clientId: "legacy-two", space: "review", status: "inactive", cleanupPending: false, createdAt: Date.now(), expiresAt: Date.now() },
    ], cursor: null });
    await connectedPanel();
    expect(await screen.findAllByText("Unrecognized app")).toHaveLength(2);
    expect(screen.queryByText(/Not used yet/)).not.toBeInTheDocument();
    expect(screen.getByText("Ended")).toBeInTheDocument();
    expect(screen.queryByText("legacy-client")).not.toBeInTheDocument();
  });

  it("follows the app language for dates and wording", async () => {
    const endsAt = Date.now() + 29 * DAY;
    mocks.listRemoteGrants.mockResolvedValue({ items: [grant({ expiresAt: endsAt })], cursor: null });
    await i18n.changeLanguage("zh-Hant");
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    panel();
    const date = new Date(endsAt).toLocaleDateString("zh-Hant");
    expect(await screen.findByText(`上次使用：5 分鐘前 · ${date} 結束`)).toBeInTheDocument();
  });

  it("pages through a long list", async () => {
    mocks.listRemoteGrants.mockResolvedValueOnce({ items: [grant()], cursor: "next" });
    await connectedPanel();
    mocks.listRemoteGrants.mockResolvedValueOnce({ items: [grant({ id: "g2", redirectHost: "chatgpt.com" })], cursor: null });
    fireEvent.click(await screen.findByRole("button", { name: "Next page" }));
    expect(await screen.findByText("ChatGPT")).toBeInTheDocument();
    expect(mocks.listRemoteGrants).toHaveBeenLastCalledWith("r1", "next");
    fireEvent.click(button("First page"));
    await waitFor(() => expect(mocks.listRemoteGrants).toHaveBeenLastCalledWith("r1", null));
  });
});

describe("when the key is running out or has run out", () => {
  it("offers Renew within 14 days, after a confirm", async () => {
    const endsAt = Date.now() + 10 * DAY;
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, credential_expires_at: endsAt });
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    panel();
    expect(await screen.findByText(`Web access ends on ${new Date(endsAt).toLocaleDateString("en")}.`)).toBeInTheDocument();
    fireEvent.click(button("Renew"));
    const ask = await screen.findByRole("group");
    expect(ask).toHaveTextContent("Keep Web access on for 90 more days? Connected web apps will need to connect again.");
    expect(mocks.renewRemoteAccess).not.toHaveBeenCalled();
    fireEvent.click(within(ask).getByRole("button", { name: "Renew" }));
    await waitFor(() => expect(mocks.renewRemoteAccess).toHaveBeenCalledWith("r1"));
    expect(mocks.toggleRemoteAccess).not.toHaveBeenCalled();
  });

  it("says nothing about renewing when the key has plenty of time", async () => {
    await connectedPanel();
    expect(screen.queryByRole("button", { name: "Renew" })).not.toBeInTheDocument();
  });

  it("shows an ended banner with 'Turn on again', and nothing that implies it still works", async () => {
    const endedAt = Date.now() - 2 * DAY;
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, credential_expires_at: endedAt });
    mocks.getRemoteAccessStatus.mockResolvedValue({ status: "error", error: "Remote connection requires device authorization" });
    mocks.listSpaces.mockResolvedValue([{ id: "s1", name: "review" }]);
    panel();
    expect(await screen.findByText(`Web access ended on ${new Date(endedAt).toLocaleDateString("en")}.`)).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByText("Connected")).not.toBeInTheDocument();
    expect(screen.queryByText("Copy this URL.")).not.toBeInTheDocument();
    expect(screen.queryByText("Connected apps")).not.toBeInTheDocument();
    expect(mocks.listRemoteGrants).not.toHaveBeenCalled();
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, enabled: false, revision: "r7" });
    fireEvent.click(button("Turn on again"));
    await waitFor(() => expect(mocks.toggleRemoteAccess.mock.calls).toEqual([[false], [true, "configured"]]));
    expect(mocks.configureRemoteAccess).toHaveBeenCalledWith("review", "r7");
  });

  it("an ended key can simply be turned off", async () => {
    mocks.getRemoteAccessProfile.mockResolvedValue({ ...profile, credential_expires_at: Date.now() - DAY });
    mocks.getRemoteAccessStatus.mockResolvedValue({ status: "off" });
    panel();
    fireEvent.click(await screen.findByRole("button", { name: "Turn off" }));
    await waitFor(() => expect(mocks.toggleRemoteAccess.mock.calls).toEqual([[false]]));
  });
});

describe("Have a code?", () => {
  it("is collapsed until asked", async () => {
    panel();
    const toggle = await screen.findByRole("button", { name: "Have a code?" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("textbox", { name: "Pairing code" })).not.toBeInTheDocument();
    fireEvent.click(toggle);
    expect(screen.getByRole("textbox", { name: "Pairing code" })).toBeInTheDocument();
  });

  it.each([
    ["abcd-2345", "ABCD2345"],
    ["  wxyz 2345 ", "WXYZ2345"],
    ["b".repeat(64), "b".repeat(64)],
  ])("hands %j to the approval dialog as %j", async (typed, parked) => {
    panel();
    fireEvent.click(await screen.findByRole("button", { name: "Have a code?" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Pairing code" }), { target: { value: typed } });
    fireEvent.click(button("Review request"));
    expect(screen.getByTestId("pending-code")).toHaveTextContent(parked);
    expect(screen.getByRole("textbox", { name: "Pairing code" })).toHaveValue("");
  });

  it("refuses a code that cannot be one, and says what a code looks like", async () => {
    panel();
    fireEvent.click(await screen.findByRole("button", { name: "Have a code?" }));
    const input = screen.getByRole("textbox", { name: "Pairing code" });
    expect(button("Review request")).toBeDisabled();
    // 0, O, 1, I, L and U are not in the code alphabet.
    fireEvent.change(input, { target: { value: "ABCD-0O1I" } });
    expect(button("Review request")).toBeDisabled();
    expect(screen.getByText("Enter the 8-character code from the web page, or the longer code from a link.")).toBeInTheDocument();
    fireEvent.submit(input.closest("form")!);
    expect(screen.getByTestId("pending-code")).toHaveTextContent("none");
  });

  it("is not offered while the saved settings cannot be read", async () => {
    mocks.getRemoteAccessProfile.mockRejectedValue(new Error("Storage unavailable"));
    panel();
    await screen.findByRole("alert");
    expect(screen.queryByRole("button", { name: "Have a code?" })).not.toBeInTheDocument();
  });
});

describe("in every language", () => {
  it.each([
    ["zh-Hant", "網頁存取", "開啟", "共用 review", "Turn on"],
    ["zh-Hans", "网页访问", "开启", "共享 review", "Turn on"],
  ])("renders %s without English fallback", async (locale, title, turnOn) => {
    await i18n.changeLanguage(locale);
    panel();
    expect(await screen.findByRole("heading", { name: title })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: turnOn })).toBeInTheDocument();
    expect(screen.queryByText(/Turn on|Web access|Space to share/)).not.toBeInTheDocument();
    cleanup();
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    panel();
    await screen.findByRole("button", { name: i18n.t("remoteAccess.testConnection") });
    expect(screen.queryByText(/Connected|Copy this URL|Sharing|Test connection|Turn off|Change/)).not.toBeInTheDocument();
  });
});

describe("the relay disclosure", () => {
  it.each(["en", "zh-Hant", "zh-Hans"])("is on screen exactly once, off or on, in %s", async (locale) => {
    await i18n.changeLanguage(locale);
    panel();
    await screen.findByRole("button", { name: i18n.t("remoteAccess.turnOn") });
    expect(screen.getAllByText(/wenlan-relay/)).toHaveLength(1);
    cleanup();
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    panel();
    await screen.findByRole("button", { name: i18n.t("remoteAccess.testConnection") });
    expect(screen.getAllByText(/wenlan-relay/)).toHaveLength(1);
  });
});

describe("how often it asks", () => {
  async function settle(rounds = 8) {
    for (let i = 0; i < rounds; i += 1) {
      await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    }
  }
  async function fakeTimerPanel() {
    vi.useFakeTimers();
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    const view = panel();
    await settle();
    expect(screen.getByText("Connected")).toBeInTheDocument();
    return view;
  }
  const calls = () => mocks.listRemoteGrants.mock.calls.length;

  it("refreshes once a minute, not every five seconds", async () => {
    await fakeTimerPanel();
    const initial = calls();
    expect(initial).toBe(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
    expect(calls()).toBe(initial);
    await act(async () => { await vi.advanceTimersByTimeAsync(31_000); });
    await settle();
    expect(calls()).toBe(initial + 1);
  });

  it("looks every five seconds while an app that was just allowed is still connecting, then slows down", async () => {
    await fakeTimerPanel();
    act(() => markPairingApproved());
    await settle();
    const before = calls();
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    await settle();
    expect(calls()).toBeGreaterThan(before);
    // The new app shows up: the fast look stops.
    mocks.listRemoteGrants.mockResolvedValue({ items: [grant({ createdAt: Date.now() + 1000 })], cursor: null });
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    await settle();
    expect(screen.getByText("Claude")).toBeInTheDocument();
    const found = calls();
    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
    expect(calls()).toBe(found);
  });

  it("gives up looking fast after two minutes if the app never connects", async () => {
    await fakeTimerPanel();
    act(() => markPairingApproved());
    await act(async () => { await vi.advanceTimersByTimeAsync(2 * 60_000 + 6000); });
    await settle();
    const later = calls();
    await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
    expect(calls()).toBe(later);
  });

  it("refreshes when the person comes back to the window", async () => {
    await fakeTimerPanel();
    const before = calls();
    await act(async () => { window.dispatchEvent(new Event("focus")); });
    await settle();
    expect(calls()).toBe(before + 1);
  });

  it("does not ask while the window is hidden, and asks once when it is shown again", async () => {
    const show = (visible: boolean) => {
      Object.defineProperty(document, "visibilityState", { configurable: true, get: () => (visible ? "visible" : "hidden") });
    };
    await fakeTimerPanel();
    try {
      show(false);
      await act(async () => { document.dispatchEvent(new Event("visibilitychange")); });
      await settle();
      const before = calls();
      await act(async () => { await vi.advanceTimersByTimeAsync(3 * 60_000); });
      expect(calls()).toBe(before);
      // A focus event that arrives while hidden is not a return to the window.
      await act(async () => { window.dispatchEvent(new Event("focus")); });
      await settle();
      expect(calls()).toBe(before);
      show(true);
      await act(async () => { document.dispatchEvent(new Event("visibilitychange")); });
      await settle();
      expect(calls()).toBe(before + 1);
    } finally {
      delete (document as { visibilityState?: unknown }).visibilityState;
    }
  });

  it("stops after an error until a manual refresh succeeds, then resumes", async () => {
    vi.useFakeTimers();
    mocks.getRemoteAccessProfile.mockResolvedValue(profile);
    mocks.getRemoteAccessStatus.mockResolvedValue(connected);
    mocks.listRemoteGrants.mockRejectedValueOnce(new Error("Remote connection unavailable; retry later"));
    panel();
    await settle();
    expect(screen.getByText("Can't reach Wenlan's web service. Check your internet connection.")).toBeInTheDocument();
    const afterError = calls();
    await act(async () => { await vi.advanceTimersByTimeAsync(3 * 60_000); });
    expect(calls()).toBe(afterError);
    fireEvent.click(button("Refresh"));
    await settle();
    expect(calls()).toBeGreaterThan(afterError);
    expect(screen.getByText("No apps connected yet")).toBeInTheDocument();
    const recovered = calls();
    await act(async () => { await vi.advanceTimersByTimeAsync(61_000); });
    await settle();
    expect(calls()).toBeGreaterThan(recovered);
  });

  it("stops entirely when Web access disconnects, and on unmount", async () => {
    const { listen } = await import("@tauri-apps/api/event");
    const view = await fakeTimerPanel();
    const handler = (listen as unknown as { mock: { calls: Array<[string, (event: { payload: unknown }) => void]> } }).mock.calls[0][1];
    mocks.getRemoteAccessStatus.mockResolvedValue({ status: "off" });
    await act(async () => { handler({ payload: { status: "off" } }); });
    await settle();
    const afterDisconnect = calls();
    await act(async () => { await vi.advanceTimersByTimeAsync(5 * 60_000); });
    expect(calls()).toBe(afterDisconnect);
    view.unmount();
    await act(async () => { await vi.advanceTimersByTimeAsync(5 * 60_000); });
    expect(calls()).toBe(afterDisconnect);
  });
});
