// SPDX-License-Identifier: AGPL-3.0-only
// UI-only synthetic fixture. No network, disk credentials or production fallback.
import type { RemoteAccessProfile, RemoteAccessStatus, RemoteGrant, RemotePairing } from "../../src/lib/tauri";
import { emit } from "./tauri-stubs";

let revision = 0;
let profile: RemoteAccessProfile | null = null;
let status: RemoteAccessStatus = { status: "off" };
const grants: RemoteGrant[] = [];
const spaces = [{ id: "review", name: "Review workspace" }, { id: "private", name: "Private library" }];
const request: RemotePairing = {
  pairingId: "a".repeat(64), clientId: "synthetic-client-for-review-0123456789",
  resource: "https://relay.wenlan.app/mcp", scopes: ["wenlan:query"], expiresAt: Date.now() + 300_000,
};

if (new URLSearchParams(window.location.search).get("remoteScenario") === "disconnect-unconfirmed") {
  profile = { revision: String(++revision), space: spaces[0].name, enabled: true, disconnect_pending: false, credential_expires_at: Date.now() + 86400_000 };
  status = { status: "error", error: "Local transport stop requested; remote access settings are not confirmed (Remote access credentials could not be stored safely); server revoke failed: Remote connection unavailable; retry later; disconnect must be retried before app restart" };
}
if (new URLSearchParams(window.location.search).get("remoteScenario") === "shutdown-unconfirmed") {
  profile = { revision: String(++revision), space: spaces[0].name, enabled: false, disconnect_pending: false, credential_expires_at: null };
  status = { status: "error", error: "Local remote-access processes have not been confirmed stopped. New connections are blocked; retry Stop access." };
}

export async function invokeRemoteFixture(command: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (command) {
    case "list_spaces": return spaces;
    case "get_remote_access_status": return { ...status };
    case "get_remote_access_profile": return profile && { ...profile };
    case "configure_remote_access": {
      if (profile?.enabled || (args?.expectedRevision ?? null) !== (profile?.revision ?? null)) throw new Error("Stale fixture revision");
      if (!spaces.some((space) => space.name === args?.space)) throw new Error("Unknown Space");
      profile = { revision: String(++revision), space: String(args?.space), enabled: false, disconnect_pending: false, credential_expires_at: null };
      return { ...profile };
    }
    case "toggle_remote_access": {
      if (args?.enabled) {
        if (!profile || profile.revision !== args.expectedRevision) throw new Error("Stale fixture revision");
        profile = { ...profile, enabled: true, revision: String(++revision), credential_expires_at: Date.now() + 86400_000 };
        status = { status: "connected", tunnel_url: null, relay_url: request.resource };
      } else {
        if (profile) profile = { ...profile, enabled: false, credential_expires_at: null, revision: String(++revision) };
        grants.length = 0;
        status = { status: "off" };
      }
      await emit("remote-access-status", { ...status });
      return { ...status };
    }
    case "inspect_remote_pairing": {
      if (args?.pairingId !== request.pairingId || args.expectedRevision !== profile?.revision) throw new Error("Unknown fixture pairing");
      return { ...request };
    }
    case "approve_remote_pairing": {
      if (!profile?.enabled || args?.expectedRevision !== profile.revision) throw new Error("Stale fixture consent");
      grants.push({ id: "g1", clientId: request.clientId, space: profile.space, createdAt: Date.now(), expiresAt: Date.now() + 86400_000, status: "active", cleanupPending: false });
      return null;
    }
    case "list_remote_grants": return { items: grants.map((grant) => ({ ...grant })), cursor: null };
    case "revoke_remote_grant": {
      const grant = grants.find((item) => item.id === args?.grantId);
      if (!grant) throw new Error("Unknown fixture grant");
      grant.status = "inactive";
      return { revoked: true, cleanupPending: false };
    }
    case "test_remote_mcp_connection": return { ok: true, latency_ms: 42, error: null };
    default: throw new Error(`Unsupported remote UI fixture command: ${command}`);
  }
}
