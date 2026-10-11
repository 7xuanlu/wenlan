// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { ArrowClockwise, ArrowSquareOut, Check, Copy } from "@phosphor-icons/react";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import {
  clipboardWrite, testRemoteMcpConnection, type RemoteConnectionTest,
} from "../../lib/tauri";
import { setPendingPairingCode } from "../../lib/pairingLink";
import { classifyPairingCode } from "../../lib/pairingCode";
import { Button, Card, Field, Input, SectionHeader, StatusChip, Tag } from "./settings/primitives";
import {
  Disclosure, InlineConfirm, RemoteErrorMessage, errorText, secondaryText,
} from "./remoteAccessParts";
import { ConnectedApps } from "./RemoteAccessApps";
import { RemoteScopeChooser } from "./RemoteScopeChooser";
import { scopeLabel, type RemoteScope } from "./remoteScope";
import { useRemoteAccess } from "./useRemoteAccess";

/** The key is offered a renewal this long before it ends. */
const EXPIRY_WARNING_MS = 14 * 24 * 60 * 60 * 1000;

type Confirming =
  | { kind: "off" }
  | { kind: "renew" }
  | { kind: "choose" }
  | { kind: "change"; scope: RemoteScope };

/** Claude's documented link that opens "Add custom connector" already filled in. */
export function claudeAddConnectorUrl(relayUrl: string): string {
  const params = new URLSearchParams({ modal: "add-custom-connector", connectorName: "Wenlan", connectorUrl: relayUrl });
  return `https://claude.ai/customize/connectors?${params.toString()}`;
}

/** ChatGPT has no link that fills in a connector, so Wenlan copies the link and opens it. */
export const CHATGPT_URL = "https://chatgpt.com/";

function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [intervalMs]);
  return now;
}

/**
 * Web access: let a web or phone AI app, like Claude or ChatGPT, search the
 * whole library or one Space of it. Turning it on is one question; each app is
 * allowed separately, in the approval dialog that opens from its own link.
 */
export function RemoteAccessPanel({ currentSpace }: { currentSpace?: string }) {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage ?? i18n.language;
  const remote = useRemoteAccess({ currentSpace });
  const { status, profile, spaces, connected, relayUrl, isOn, pendingDisconnect, ready, nativeReadFailed, nativeLoading, queryError, scope } = remote;
  const now = useNow(60_000);
  const [confirming, setConfirming] = useState<Confirming | null>(null);
  const [copied, setCopied] = useState(false);
  const [chatgptOpened, setChatgptOpened] = useState(false);
  const [probe, setProbe] = useState<RemoteConnectionTest | null>(null);

  const spaceExists = (target: RemoteScope) => target.kind === "wholeLibrary" || spaces.some((item) => item.name === target.name);
  const expiresAt = profile?.enabled ? profile.credential_expires_at : null;
  const ended = expiresAt !== null && expiresAt !== undefined && expiresAt <= now;
  const expiringSoon = expiresAt !== null && expiresAt !== undefined && !ended && expiresAt - now <= EXPIRY_WARNING_MS;
  const stuck = status?.status === "error" && !profile?.enabled;
  const spacesFailed = !nativeReadFailed && Boolean(queryError);
  const revision = profile?.revision;

  // A different saved state is a different situation: forget stale answers.
  useEffect(() => {
    setConfirming(null);
    setProbe(null);
  }, [revision]);

  const action = useMutation({
    mutationFn: (operation: () => Promise<void>) => operation(),
    onSettled: () => remote.refresh(),
  });
  const busy = action.isPending;
  const run = (operation: () => Promise<void>) => { action.reset(); action.mutate(operation); };

  const turnOn = (target: RemoteScope) => run(async () => {
    if (!ready || pendingDisconnect || !spaceExists(target)) return;
    await remote.switchOn(target);
  });
  const turnOff = () => run(async () => { await remote.stop(); setConfirming(null); });
  // The same scope again. A Space that is gone is never swapped for another
  // one or widened: Web access turns off and asks again.
  const turnOnAgain = () => run(async () => {
    await remote.stop();
    if (scope && spaceExists(scope)) await remote.switchOn(scope);
  });
  const changeScope = (target: RemoteScope) => run(async () => {
    await remote.stop();
    await remote.switchOn(target);
    setConfirming(null);
  });
  const renew = () => run(async () => { await remote.renew(profile!.revision); setConfirming(null); });
  const testConnection = () => run(async () => {
    setProbe(null);
    setProbe(await testRemoteMcpConnection());
  });
  const copyUrl = (url: string) => run(async () => { await clipboardWrite(url); setCopied(true); });
  const addToClaude = (url: string) => run(async () => { await openExternal(claudeAddConnectorUrl(url)); });
  const addToChatGPT = (url: string) => run(async () => {
    await clipboardWrite(url);
    setCopied(true);
    setChatgptOpened(true);
    await openExternal(CHATGPT_URL);
  });

  const stopButton = (
    <Button variant="secondary" size="sm" disabled={busy} onClick={() => run(async () => { await remote.stop(); })}>
      {t("remoteAccess.stopAccess")}
    </Button>
  );

  let body: React.ReactNode;
  if (nativeLoading) {
    body = <p role="status" className={secondaryText}>{t("settings.controlState.loading")}</p>;
  } else if (nativeReadFailed) {
    // The saved settings could not be read, so nothing here may look like permission to turn on.
    body = <div className="space-y-2"><RemoteErrorMessage error={queryError} />{stopButton}</div>;
  } else if (pendingDisconnect) {
    body = (
      <div className="space-y-2">
        <p role="status" className={secondaryText}>{t("remoteAccess.disconnectPending")}</p>
        <Button variant="secondary" size="sm" disabled={busy} onClick={() => run(async () => { await remote.stop(); })}>
          {t("remoteAccess.retryDisconnect")}
        </Button>
      </div>
    );
  } else if (stuck && status?.status === "error") {
    body = <div className="space-y-2"><RemoteErrorMessage error={status.error} />{stopButton}</div>;
  } else if (!isOn) {
    body = (
      <div className="min-w-0 space-y-3">
        <StatusChip state={{ kind: "idle" }} label={t("remoteAccess.statusOff")} />
        {spacesFailed
          ? <RemoteErrorMessage error={queryError} />
          : <RemoteScopeChooser spaces={spaces} defaultSpace={remote.defaultSpace} busy={busy} disabled={!ready} onChoose={turnOn} />}
      </div>
    );
  } else if (ended) {
    body = (
      <div className="min-w-0 space-y-3">
        <div role="status" className="space-y-2 rounded border border-[var(--mem-status-warning-border)] bg-[var(--mem-status-warning-bg)] p-3 text-sm text-[var(--mem-status-warning-text)]">
          <p className="break-words">{t("remoteAccess.endedBanner", { date: new Date(expiresAt!).toLocaleDateString(language) })}</p>
          <div className="flex flex-wrap gap-2">
            <Button variant="primary" size="sm" loading={busy} disabled={!ready} onClick={turnOnAgain}>{t("remoteAccess.turnOnAgain")}</Button>
            <Button variant="secondary" size="sm" disabled={busy} onClick={() => run(async () => { await remote.stop(); })}>{t("remoteAccess.turnOff")}</Button>
          </div>
        </div>
      </div>
    );
  } else {
    const failed = status?.status === "error";
    body = (
      <div className="min-w-0 space-y-4">
        <div className="flex flex-wrap items-center gap-3">
          {connected && <StatusChip state={{ kind: "up" }} label={t("remoteAccess.statusConnected")} />}
          {status?.status === "starting" && <StatusChip state={{ kind: "probing" }} label={t("remoteAccess.statusConnecting")} />}
          {!connected && status?.status === "off" && <StatusChip state={{ kind: "idle" }} label={t("remoteAccess.statusOff")} />}
          {profile && <span className="text-sm break-words">{t("remoteAccess.sharing", { space: scopeLabel(t, profile.space) })}</span>}
        </div>

        {failed && <RemoteErrorMessage error={status.error} />}

        {expiringSoon && (
          <div role="status" className="space-y-2 rounded border border-[var(--mem-status-warning-border)] bg-[var(--mem-status-warning-bg)] p-3 text-sm text-[var(--mem-status-warning-text)]">
            <p className="break-words">{t("remoteAccess.expiringSoon", { date: new Date(expiresAt!).toLocaleDateString(language) })}</p>
            {confirming?.kind === "renew"
              ? <InlineConfirm message={t("remoteAccess.renewConfirm")} confirmLabel={t("remoteAccess.renew")} busy={busy}
                  onConfirm={renew} onCancel={() => setConfirming(null)} />
              : <Button variant="secondary" size="sm" disabled={busy} onClick={() => setConfirming({ kind: "renew" })}>{t("remoteAccess.renew")}</Button>}
          </div>
        )}

        <div className="flex flex-wrap items-center gap-2">
          {profile?.enabled && (
            <Button variant="secondary" size="sm" disabled={busy || !ready} onClick={() => run(async () => { await remote.reconnect(profile.revision); })}>
              <ArrowClockwise size={14} aria-hidden="true" />{t("remoteAccess.reconnect")}
            </Button>
          )}
          {connected && (
            <Button variant="secondary" size="sm" disabled={busy} onClick={testConnection}>
              {t("remoteAccess.testConnection")}
            </Button>
          )}
          {failed
            ? stopButton
            : <>
                <Button variant="secondary" size="sm" aria-expanded={confirming?.kind === "choose" || confirming?.kind === "change"}
                  disabled={busy || spacesFailed || !ready || (scope?.kind === "wholeLibrary" && spaces.length === 0)}
                  onClick={() => setConfirming({ kind: "choose" })}>{t("remoteAccess.changeSpace")}</Button>
                <Button variant="secondary" size="sm" disabled={busy} onClick={() => setConfirming({ kind: "off" })}>{t("remoteAccess.turnOff")}</Button>
              </>}
        </div>

        {probe?.ok && <p role="status" className={secondaryText}>{t("remoteAccess.testOk", { ms: probe.latency_ms ?? "?" })}</p>}
        {probe && !probe.ok && (
          <div className="space-y-1">
            <p role="alert" className={errorText}>{t("remoteAccess.connectionFailed")}</p>
            {probe.error && <Disclosure label={t("remoteAccess.details")}><p className="text-xs break-words text-[var(--mem-text-secondary)]">{probe.error}</p></Disclosure>}
          </div>
        )}

        {confirming?.kind === "off" && (
          <InlineConfirm message={t("remoteAccess.offConfirm")} confirmLabel={t("remoteAccess.turnOff")} busy={busy}
            onConfirm={turnOff} onCancel={() => setConfirming(null)} />
        )}
        {confirming?.kind === "choose" && (
          <div className="min-w-0 space-y-2">
            <RemoteScopeChooser spaces={spaces} defaultSpace={remote.defaultSpace} current={scope} busy={busy}
              onChoose={(target) => setConfirming({ kind: "change", scope: target })} />
            <Button variant="ghost" size="sm" onClick={() => setConfirming(null)}>{t("remoteAccess.cancel")}</Button>
          </div>
        )}
        {confirming?.kind === "change" && (
          <InlineConfirm message={confirming.scope.kind === "wholeLibrary"
            ? t("remoteAccess.changeConfirmWholeLibrary")
            : t("remoteAccess.changeConfirm", { space: confirming.scope.name })} confirmLabel={t("remoteAccess.change")}
            busy={busy} onConfirm={() => changeScope(confirming.scope)} onCancel={() => setConfirming(null)} />
        )}

        {connected && relayUrl && (
          <div className="border-t border-[var(--mem-border)] pt-4 space-y-2">
            <ol className="list-decimal pl-5 space-y-2 text-sm">
              <li className="min-w-0 space-y-2">
                <span>{t("remoteAccess.howTo1")}</span>
                <span className="flex flex-wrap gap-2">
                  <Button variant="primary" size="sm" disabled={busy} onClick={() => addToClaude(relayUrl)}>
                    {t("remoteAccess.addToClaude")}<ArrowSquareOut size={14} aria-hidden="true" />
                  </Button>
                  <Button variant="secondary" size="sm" disabled={busy} onClick={() => addToChatGPT(relayUrl)}>
                    {t("remoteAccess.addToChatGPT")}<ArrowSquareOut size={14} aria-hidden="true" />
                  </Button>
                </span>
                {chatgptOpened && <p role="status" className={secondaryText}>{t("remoteAccess.chatgptHint")}</p>}
                <span className="block text-[var(--mem-text-secondary)]">{t("remoteAccess.otherAppsHint")}</span>
                <span className="flex items-start gap-2 min-w-0">
                  <code className="flex-1 min-w-0 break-all text-xs py-1">{relayUrl}</code>
                  <button type="button" className="p-2 shrink-0 rounded border border-[var(--mem-border)]"
                    title={t("connectMatrix.copyUrl")} aria-label={copied ? t("remoteAccess.linkCopied") : t("connectMatrix.copyUrl")}
                    disabled={busy} onClick={() => copyUrl(relayUrl)}>
                    {copied ? <Check size={16} /> : <Copy size={16} />}
                  </button>
                </span>
              </li>
              <li>{t("remoteAccess.howTo2")}</li>
            </ol>
            <p className={secondaryText}>{t("remoteAccess.localAppsHint")}</p>
          </div>
        )}

        {/* Only while connected: asking the relay otherwise would only fail, and an empty header reads as broken. */}
        {profile?.enabled && connected && Boolean(profile.credential_expires_at) && (
          <div className="border-t border-[var(--mem-border)] pt-4">
            <ConnectedApps key={profile.revision} revision={profile.revision} enabled />
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="min-w-0 space-y-4" style={{ fontFamily: "var(--mem-font-body)", color: "var(--mem-text)" }}>
      <SectionHeader label={t("remoteAccess.title")} action={<Tag tone="neutral">{t("remoteAccess.experimentalBadge")}</Tag>} />
      <Card padding="card">
        <div className="min-w-0 space-y-4">
          <p className={secondaryText}>{t("remoteAccess.intro")}</p>
          {/* Said once, always: requests and results go through the relay. */}
          <p className={secondaryText}>{t("remoteAccess.relayDisclosure")}</p>
          {body}
          {action.error && <RemoteErrorMessage error={action.error} />}
          {!nativeLoading && !nativeReadFailed && !pendingDisconnect && !stuck && !ended && <CodeEntry />}
        </div>
      </Card>
    </div>
  );
}

/** For when the browser's link did not open Wenlan: type what the page shows. */
function CodeEntry() {
  const { t } = useTranslation();
  const id = useId();
  const [value, setValue] = useState("");
  const classified = classifyPairingCode(value);
  const invalid = value.trim() !== "" && classified.kind === "invalid";
  return (
    <div className="border-t border-[var(--mem-border)] pt-4">
      <Disclosure label={t("remoteAccess.haveCode")}>
        <form className="space-y-3 max-w-sm" onSubmit={(event) => {
          event.preventDefault();
          if (classified.kind === "invalid") return;
          // The approval dialog does the rest, the same as for a link.
          setPendingPairingCode(classified.code);
          setValue("");
        }}>
          <p className={secondaryText}>{t("remoteAccess.codeHint")}</p>
          <Field label={t("remoteAccess.pairingCode")} htmlFor={id} error={invalid ? t("remoteAccess.codeInvalid") : undefined}>
            <Input value={value} invalid={invalid} mono maxLength={80} autoComplete="off" spellCheck={false}
              onChange={(event) => setValue(event.target.value)} />
          </Field>
          <Button type="submit" variant="secondary" size="sm" disabled={classified.kind === "invalid"}>{t("remoteAccess.inspectPairing")}</Button>
        </form>
      </Disclosure>
    </div>
  );
}
