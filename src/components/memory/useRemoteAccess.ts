// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  configureRemoteAccess, getRemoteAccessProfile, getRemoteAccessStatus, listSpaces,
  reconnectRemoteAccess, renewRemoteAccess, toggleRemoteAccess,
  type RemoteAccessStatus, type RemoteScope,
} from "../../lib/tauri";
import { WHOLE_LIBRARY, savedScope } from "./remoteScope";

export const REMOTE_STATUS = ["remote-access-status"] as const;
export const REMOTE_PROFILE = ["remote-access-profile"] as const;
export const REMOTE_GRANTS = ["remote-access-grants"] as const;

/**
 * Web access as the native side reports it, plus the few things that change it.
 * The settings panel and the approval dialog both read this, so they never
 * disagree about whether Web access is on.
 *
 * `active: false` keeps a closed dialog from asking the native side anything.
 */
export function useRemoteAccess({ active = true, currentSpace }: { active?: boolean; currentSpace?: string } = {}) {
  const cache = useQueryClient();
  const statusQuery = useQuery({ queryKey: REMOTE_STATUS, queryFn: getRemoteAccessStatus, enabled: active });
  const profileQuery = useQuery({ queryKey: REMOTE_PROFILE, queryFn: getRemoteAccessProfile, enabled: active });
  const spacesQuery = useQuery({ queryKey: ["spaces"], queryFn: listSpaces, enabled: active });
  const status = statusQuery.data;
  const profile = profileQuery.data;
  // `*` is the whole-library value; it is never offered as a Space.
  const spaces = (spacesQuery.data ?? []).filter((item) => item.name !== WHOLE_LIBRARY);

  useEffect(() => {
    if (!active) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<RemoteAccessStatus>("remote-access-status", ({ payload }) => {
      cache.setQueryData(REMOTE_STATUS, payload);
      void cache.invalidateQueries({ queryKey: REMOTE_PROFILE });
      void cache.invalidateQueries({ queryKey: REMOTE_GRANTS });
    }).then((stop) => {
      if (disposed) stop(); else unlisten = stop;
    }).catch(() => { void cache.invalidateQueries({ queryKey: REMOTE_STATUS }); });
    return () => { disposed = true; unlisten?.(); };
  }, [cache, active]);

  const connected = status?.status === "connected";
  const savedSpace = profile && spaces.some((item) => item.name === profile.space) ? profile.space : "";
  // The Space already shared, else the one the person is looking at, else the
  // first one. It is always shown before anything is turned on.
  const defaultSpace = savedSpace
    || (currentSpace && spaces.some((item) => item.name === currentSpace) ? currentSpace : spaces[0]?.name ?? "");

  const refresh = async () => {
    await Promise.all([
      cache.invalidateQueries({ queryKey: REMOTE_PROFILE }),
      cache.invalidateQueries({ queryKey: REMOTE_STATUS }),
      cache.invalidateQueries({ queryKey: REMOTE_GRANTS }),
    ]);
  };

  return {
    status,
    profile,
    spaces,
    connected,
    relayUrl: status?.status === "connected" ? status.relay_url : null,
    isOn: Boolean(profile?.enabled || status?.status === "starting" || connected),
    pendingDisconnect: Boolean(profile?.disconnect_pending),
    ready: profileQuery.isSuccess && statusQuery.isSuccess && spacesQuery.isSuccess,
    nativeReadFailed: profileQuery.isError || statusQuery.isError,
    nativeLoading: profileQuery.isPending || statusQuery.isPending,
    queryError: profileQuery.error ?? statusQuery.error ?? spacesQuery.error,
    defaultSpace,
    /** What the saved profile shares, or null before anything is chosen. */
    scope: savedScope(profile?.space),
    refresh,
    /**
     * Saves what to share and turns Web access on. The saved settings are read
     * again first: turning off just changed their revision, and a stale one is refused.
     */
    async switchOn(scope: RemoteScope) {
      const fresh = await getRemoteAccessProfile();
      const saved = await configureRemoteAccess(scope, fresh?.revision);
      cache.setQueryData(REMOTE_STATUS, await toggleRemoteAccess(true, saved.revision));
    },
    async stop() {
      cache.setQueryData(REMOTE_STATUS, await toggleRemoteAccess(false));
    },
    async reconnect(revision: string) {
      cache.setQueryData(REMOTE_STATUS, await reconnectRemoteAccess(revision));
    },
    async renew(revision: string) {
      cache.setQueryData(REMOTE_STATUS, await renewRemoteAccess(revision));
    },
  };
}
