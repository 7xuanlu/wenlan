// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Copy, ArrowClockwise, Check } from "@phosphor-icons/react";
import {
  approveRemotePairing, clipboardWrite, configureRemoteAccess, getRemoteAccessProfile,
  getRemoteAccessStatus, inspectRemotePairing, listRemoteGrants, listSpaces,
  revokeRemoteGrant, testRemoteMcpConnection, toggleRemoteAccess,
  type RemoteAccessStatus, type RemotePairing, type RemoteGrantPage,
} from "../../lib/tauri";
import { Button, StatusChip, Tag, Toggle } from "./settings/primitives";

const STATUS = ["remote-access-status"] as const;
const PROFILE = ["remote-access-profile"] as const;
const GRANTS = ["remote-access-grants"] as const;
const fieldClass = "w-full min-w-0 rounded border px-3 py-2 bg-[var(--mem-bg)] border-[var(--mem-border)] text-[var(--mem-text)]";
const secondary = "text-[var(--mem-text-secondary)] text-sm";
const errorClass = "text-sm text-[var(--mem-status-danger-text)] break-words";

export function RemoteAccessPanel({ currentSpace }: { currentSpace?: string }) {
  const { t } = useTranslation();
  const id = useId();
  const cache = useQueryClient();
  const statusQuery = useQuery({ queryKey: STATUS, queryFn: getRemoteAccessStatus });
  const profileQuery = useQuery({ queryKey: PROFILE, queryFn: getRemoteAccessProfile });
  const spacesQuery = useQuery({ queryKey: ["spaces"], queryFn: listSpaces });
  const status = statusQuery.data;
  const profile = profileQuery.data;
  const spaces = spacesQuery.data ?? [];
  const [selected, setSelected] = useState<string | null>(null);
  const [consented, setConsented] = useState(false);
  const [pairingId, setPairingId] = useState("");
  const [inspection, setInspection] = useState<{ request: RemotePairing; revision: string } | null>(null);
  const [approved, setApproved] = useState(false);
  const [copied, setCopied] = useState(false);
  const [probe, setProbe] = useState<string | null>(null);
  const [cursor, setCursor] = useState<string | null>(null);
  const [grantNotice, setGrantNotice] = useState<string | null>(null);
  const implicitSpace = currentSpace && spaces.some((space) => space.name === currentSpace)
    ? currentSpace : spaces.length === 1 ? spaces[0].name : "";
  const space = selected ?? profile?.space ?? implicitSpace;
  const connected = status?.status === "connected";
  const publicMcp = status?.status === "connected" ? status.relay_url : null;
  const isOn = Boolean(profile?.enabled || status?.status === "starting" || connected);
  const pending = Boolean(profile?.disconnect_pending);
  const ready = profileQuery.isSuccess && statusQuery.isSuccess && spacesQuery.isSuccess;
  const scopeExists = spaces.some((item) => item.name === space);
  const grantQuery = useQuery({
    queryKey: [...GRANTS, profile?.revision, cursor],
    queryFn: () => listRemoteGrants(profile!.revision, cursor),
    enabled: connected && Boolean(profile?.enabled && profile.credential_expires_at),
    retry: false,
  });

  useEffect(() => { setConsented(false); }, [space]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<RemoteAccessStatus>("remote-access-status", ({ payload }) => {
      cache.setQueryData(STATUS, payload);
      void cache.invalidateQueries({ queryKey: PROFILE });
    }).then((stop) => {
      if (disposed) stop(); else unlisten = stop;
    }).catch(() => { void cache.invalidateQueries({ queryKey: STATUS }); });
    return () => { disposed = true; unlisten?.(); };
  }, [cache]);

  useEffect(() => {
    setInspection(null);
    setApproved(false);
    setCursor(null);
    setProbe(null);
    setGrantNotice(null);
  }, [profile?.revision]);

  const action = useMutation({
    mutationFn: (operation: () => Promise<void>) => operation(),
    onSettled: async () => {
      await cache.invalidateQueries({ queryKey: PROFILE });
      await cache.invalidateQueries({ queryKey: STATUS });
      await cache.invalidateQueries({ queryKey: GRANTS });
    },
  });

  const enable = async () => {
    if (!ready || !consented || !scopeExists || pending) throw new Error(t("remoteAccess.scopeRequired"));
    const saved = await configureRemoteAccess(space, profile?.revision);
    const next = await toggleRemoteAccess(true, saved.revision);
    cache.setQueryData(STATUS, next);
    setConsented(false);
  };
  const stop = async () => {
    cache.setQueryData(STATUS, await toggleRemoteAccess(false));
    setInspection(null);
  };
  const reconnect = async () => {
    const previousSpace = profile?.space;
    await stop();
    const fresh = await getRemoteAccessProfile();
    if (!fresh || fresh.space !== previousSpace || fresh.disconnect_pending) throw new Error(t("remoteAccess.scopeRequired"));
    cache.setQueryData(STATUS, await toggleRemoteAccess(true, fresh.revision));
  };
  const inspect = async () => {
    setApproved(false);
    setInspection(null);
    if (!profile) throw new Error(t("remoteAccess.scopeRequired"));
    const revision = profile.revision;
    const request = await inspectRemotePairing(revision, pairingId.trim());
    setInspection({ revision, request });
  };
  const approve = async () => {
    if (!inspection || inspection.revision !== profile?.revision || inspection.request.expiresAt <= Date.now()) {
      throw new Error(t("remoteAccess.pairingExpired"));
    }
    await approveRemotePairing(inspection.revision, inspection.request);
    setInspection(null);
    setPairingId("");
    setApproved(true);
  };
  const queryError = profileQuery.error ?? statusQuery.error ?? spacesQuery.error;
  const busy = action.isPending;

  return (
    <div className="min-w-0 space-y-4" style={{ fontFamily: "var(--mem-font-body)", color: "var(--mem-text)" }}>
      <div className="flex items-start justify-between gap-4">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="text-base font-semibold">{t("remoteAccess.title")}</h3>
          <Tag tone="accent">{t("remoteAccess.experimentalBadge")}</Tag>
        </div>
        <fieldset disabled={busy || (!isOn && (!ready || pending || !consented || !scopeExists))}>
          <Toggle enabled={isOn} valueUnknown={!profileQuery.isSuccess || !statusQuery.isSuccess}
            onToggle={() => action.mutate(isOn ? stop : enable)}
            aria-label={t("remoteAccess.title")} aria-describedby={id + "-consent"} />
        </fieldset>
      </div>
      <p id={id + "-consent"} className={secondary}>{t("remoteAccess.consentDisclosure")}</p>
      <fieldset disabled={busy || isOn || pending || !ready} className="min-w-0 space-y-2">
        <label htmlFor={id + "-space"} className="block text-sm font-medium">{t("remoteAccess.dataScope")}</label>
        {spaces.length === 1 && scopeExists
          ? <p className="text-sm break-words">{space}</p>
          : <select id={id + "-space"} className={fieldClass} value={space}
              onChange={(event) => { setSelected(event.target.value); setConsented(false); }}>
              <option value="">{t("remoteAccess.chooseSpace")}</option>
              {spaces.map((item) => <option key={item.id} value={item.name}>{item.name}</option>)}
            </select>}
        {!isOn && !pending && <label className="flex items-start gap-2 text-sm">
          <input type="checkbox" checked={consented} disabled={!scopeExists}
            onChange={(event) => setConsented(event.target.checked)} className="mt-1 shrink-0" />
          <span>{scopeExists
            ? t("remoteAccess.scopeConsent", { space })
            : t("remoteAccess.scopeConsentPending")}</span>
        </label>}
      </fieldset>
      {queryError && <p role="alert" className={errorClass}>{String(queryError)}</p>}
      {action.error && <p role="alert" className={errorClass}>{String(action.error)}</p>}
      {status?.status === "starting" && <StatusChip state={{ kind: "probing" }} label={t("remoteAccess.statusConnecting")} />}
      {status?.status === "error" && <p role="alert" className={errorClass}>{status.error}</p>}
      {(queryError || (status?.status === "error" && !pending)) &&
        <Button variant="secondary" size="sm" disabled={busy} onClick={() => action.mutate(stop)}>{t("remoteAccess.stopAccess")}</Button>}
      {pending && <div className="space-y-2">
        <p role="status" className={secondary}>{t("remoteAccess.disconnectPending")}</p>
        <Button variant="secondary" size="sm" disabled={busy} onClick={() => action.mutate(stop)}>{t("remoteAccess.retryDisconnect")}</Button>
      </div>}
      {isOn && <div className="flex flex-wrap items-center gap-3">
        {connected && <StatusChip state={{ kind: "up" }} label={t("remoteAccess.transportConnected")} />}
        <Button variant="secondary" size="sm" disabled={busy} onClick={() => action.mutate(reconnect)}>
          <ArrowClockwise size={14} aria-hidden="true" />{t("remoteAccess.reconnect")}
        </Button>
        {connected && <Button variant="secondary" size="sm" disabled={busy} onClick={() => action.mutate(async () => {
          setProbe(null);
          const result = await testRemoteMcpConnection();
          if (!result.ok) throw new Error(result.error ?? t("remoteAccess.connectionFailed"));
          setProbe(t("remoteAccess.backendVerified", { ms: result.latency_ms ?? "?" }));
        })}>{t("remoteAccess.testConnection")}</Button>}
        {probe && <span role="status" className={secondary}>{probe}</span>}
      </div>}
      {connected && profile?.enabled && <>
        {publicMcp && <div className="border-t border-[var(--mem-border)] pt-4 space-y-2">
          <h4 className="text-sm font-semibold">{t("remoteAccess.endpoint")}</h4>
          <div className="flex items-start gap-2 min-w-0">
            <code className="flex-1 min-w-0 break-all text-xs py-2">{publicMcp}</code>
            <button type="button" className="p-2 shrink-0 rounded border border-[var(--mem-border)]"
              title={t("connectMatrix.copyUrl")} aria-label={t("connectMatrix.copyUrl")}
              disabled={busy} onClick={() => action.mutate(async () => { await clipboardWrite(publicMcp); setCopied(true); })}>
              {copied ? <Check size={16} /> : <Copy size={16} />}
            </button>
          </div>
        </div>}
        <div className="border-t border-[var(--mem-border)] pt-4 space-y-3">
          <h4 className="text-sm font-semibold">{t("remoteAccess.pairingTitle")}</h4>
          <label htmlFor={id + "-pairing"} className="block text-sm">{t("remoteAccess.pairingCode")}</label>
          <div className="flex flex-wrap gap-2">
            <input id={id + "-pairing"} className={fieldClass + " flex-1 basis-48"} value={pairingId}
              maxLength={64} autoComplete="off" spellCheck={false} disabled={busy}
              onChange={(event) => { setPairingId(event.target.value); setInspection(null); setApproved(false); }} />
            <Button variant="secondary" size="sm" disabled={busy || !/^[a-zA-Z0-9_-]{64}$/.test(pairingId.trim())}
              onClick={() => action.mutate(inspect)}>{t("remoteAccess.inspectPairing")}</Button>
          </div>
          {inspection && inspection.revision === profile.revision && <div className="space-y-2">
            <dl className="text-sm space-y-1">
              <dt className={secondary}>{t("remoteAccess.clientId")}</dt>
              <dd className="break-all font-mono text-xs">{inspection.request.clientId}</dd>
              <dt className={secondary}>{t("remoteAccess.dataScope")}</dt><dd className="break-words">{profile.space}</dd>
              <dt className={secondary}>{t("remoteAccess.expires")}</dt><dd>{new Date(inspection.request.expiresAt).toLocaleString()}</dd>
            </dl>
            <p className={secondary}>{t("remoteAccess.approvalDisclosure", { space: profile.space })}</p>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" disabled={busy} onClick={() => action.mutate(approve)}>{t("remoteAccess.approvePairing")}</Button>
              <Button variant="secondary" size="sm" disabled={busy} onClick={() => { setInspection(null); setPairingId(""); }}>{t("common.close")}</Button>
            </div>
          </div>}
          {approved && <p role="status" className={secondary}>{t("remoteAccess.pairingApproved")}</p>}
        </div>
        <div className="border-t border-[var(--mem-border)] pt-4 space-y-3">
          <div className="flex items-center justify-between gap-2">
            <h4 className="text-sm font-semibold">{t("remoteAccess.authorizedClients")}</h4>
            <button type="button" className="p-2 rounded" disabled={busy || grantQuery.isFetching}
              aria-label={t("remoteAccess.refreshGrants")} title={t("remoteAccess.refreshGrants")}
              onClick={() => { void grantQuery.refetch(); }}><ArrowClockwise size={16} /></button>
          </div>
          {grantQuery.isPending && <p className={secondary}>{t("remoteAccess.loadingGrants")}</p>}
          {grantQuery.error && <p role="alert" className={errorClass}>{String(grantQuery.error)}</p>}
          {grantQuery.data?.items.length === 0 && <p className={secondary}>{t("remoteAccess.noGrants")}</p>}
          <ul className="divide-y divide-[var(--mem-border)]">
            {grantQuery.data?.items.map((grant) => <li key={grant.id} className="py-3 flex flex-wrap items-start gap-3">
              <div className="flex-1 min-w-0 basis-40 space-y-1">
                <p className="font-mono text-xs break-all">{grant.clientId}</p>
                <p className={secondary + " break-words"}>{grant.space}</p>
                <p className={secondary}>{t(grant.status === "active" ? "remoteAccess.grantActive" : "remoteAccess.grantRevoked")}</p>
                {grant.cleanupPending && <p className={secondary}>{t("remoteAccess.cleanupPending")}</p>}
              </div>
              {(grant.status === "active" || grant.cleanupPending) && <Button variant="secondary" size="sm" disabled={busy}
                onClick={() => action.mutate(async () => {
                  const result = await revokeRemoteGrant(profile.revision, grant.id);
                  cache.setQueryData<RemoteGrantPage>([...GRANTS, profile.revision, cursor], (page) => page && ({
                    ...page,
                    items: page.items.map((item) => item.id === grant.id
                      ? { ...item, status: "inactive", cleanupPending: result.cleanupPending } : item),
                  }));
                  setApproved(false);
                  setGrantNotice(t(result.cleanupPending ? "remoteAccess.cleanupPending" : "remoteAccess.grantRevoked"));
                })}>{t(grant.status === "active" ? "remoteAccess.revokeGrant" : "remoteAccess.retry")}</Button>}
            </li>)}
          </ul>
          {grantNotice && <p role="status" className={secondary}>{grantNotice}</p>}
          <div className="flex gap-2">
            {cursor && <Button variant="secondary" size="sm" disabled={busy} onClick={() => setCursor(null)}>{t("remoteAccess.firstPage")}</Button>}
            {grantQuery.data?.cursor && <Button variant="secondary" size="sm" disabled={busy} onClick={() => setCursor(grantQuery.data!.cursor)}>{t("remoteAccess.nextPage")}</Button>}
          </div>
        </div>
      </>}
    </div>
  );
}
