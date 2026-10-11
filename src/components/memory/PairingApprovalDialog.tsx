// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  approveRemotePairing, denyRemotePairing, inspectRemotePairing, lookupRemotePairing,
  type RemotePairing,
} from "../../lib/tauri";
import {
  clearPendingPairingCode, holdPendingPairingCode, markPairingApproved, usePendingPairingCode,
} from "../../lib/pairingLink";
import { classifyPairingCode } from "../../lib/pairingCode";
import { clientIdentity } from "../../lib/remoteClient";
import { describeRemoteError } from "../../lib/remoteErrors";
import { Button, Field, Select } from "./settings/primitives";
import { RemoteErrorMessage } from "./remoteAccessParts";
import { REMOTE_GRANTS, useRemoteAccess } from "./useRemoteAccess";
import "./pairingApproval.css";

type Step =
  | { kind: "loading" }
  | { kind: "review"; request: RemotePairing; revision: string }
  | { kind: "working"; request: RemotePairing; doing: "allow" | "deny" }
  | { kind: "approved"; request: RemotePairing }
  | { kind: "expired" }
  | { kind: "error"; error: unknown; context: "pairing" | "other" };

/**
 * Asks whether an AI app may search the library. It opens from anywhere: a
 * `wenlan://pair` link or a typed code parks a pairing code, and this shows it.
 * Nothing is allowed without a click on Allow, and the app is named from the
 * host it asked to be sent back to, never from the name it gave itself.
 */
export default function PairingApprovalDialog({ currentSpace }: { currentSpace?: string }) {
  const code = usePendingPairingCode();
  // A different code is a different request: start it from scratch.
  return code ? <ApprovalDialog key={code} code={code} currentSpace={currentSpace} /> : null;
}

function ApprovalDialog({ code, currentSpace }: { code: string; currentSpace?: string }) {
  const { t } = useTranslation();
  const cache = useQueryClient();
  const remote = useRemoteAccess({ currentSpace });
  const { profile, status, connected } = remote;
  const parsed = classifyPairingCode(code);
  const panel = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const bodyId = useId();
  const spaceId = useId();
  const [chosen, setChosen] = useState<string | null>(null);
  const [step, setStep] = useState<Step>({ kind: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [turningOn, setTurningOn] = useState(false);
  const [turnOnError, setTurnOnError] = useState<unknown>(null);
  // Once a decision is on its way, a refreshed profile must not start over.
  const decided = useRef(false);

  const space = chosen ?? remote.defaultSpace;
  const revision = profile?.revision;
  const canLookUp = connected && Boolean(profile?.enabled) && !remote.pendingDisconnect && parsed.kind !== "invalid";

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panel.current?.focus();
    return () => { if (previous?.isConnected) previous.focus(); };
  }, []);

  useEffect(() => {
    if (!canLookUp || !revision || decided.current) return;
    let current = true;
    setStep({ kind: "loading" });
    const found = parsed.kind === "short"
      ? lookupRemotePairing(revision, parsed.code)
      : inspectRemotePairing(revision, parsed.code);
    found
      .then((request) => { if (current) setStep({ kind: "review", request, revision }); })
      .catch((error) => {
        if (!current) return;
        setStep(describeRemoteError(error, "pairing").kind === "expired"
          ? { kind: "expired" } : { kind: "error", error, context: "pairing" });
      });
    return () => { current = false; };
  }, [canLookUp, revision, parsed.kind, parsed.kind === "invalid" ? "" : parsed.code, attempt]);

  // Keep this request while it is being looked up or decided. Before that (Web
  // access off, a bad code) or once it has an outcome, a newer link replaces it.
  const holding = step.kind === "review" || step.kind === "working"
    || (step.kind === "loading" && canLookUp);
  useEffect(() => {
    holdPendingPairingCode(holding);
    return () => holdPendingPairingCode(false);
  }, [holding]);

  // A request the person never answers goes stale on the relay; say so.
  const reviewExpiresAt = step.kind === "review" ? step.request.expiresAt : null;
  useEffect(() => {
    if (reviewExpiresAt === null) return;
    const wait = reviewExpiresAt - Date.now();
    if (wait <= 0) { setStep({ kind: "expired" }); return; }
    const timer = window.setTimeout(() => setStep({ kind: "expired" }), wait);
    return () => window.clearTimeout(timer);
  }, [reviewExpiresAt]);

  const close = () => { clearPendingPairingCode(); };

  const allow = async () => {
    if (step.kind !== "review") return;
    const { request, revision: inspectedAt } = step;
    if (request.expiresAt <= Date.now()) { setStep({ kind: "expired" }); return; }
    if (inspectedAt !== profile?.revision) { setAttempt((value) => value + 1); return; }
    decided.current = true;
    setStep({ kind: "working", request, doing: "allow" });
    try {
      await approveRemotePairing(inspectedAt, request);
      markPairingApproved();
      void cache.invalidateQueries({ queryKey: REMOTE_GRANTS });
      setStep({ kind: "approved", request });
    } catch (error) {
      decided.current = false;
      setStep(describeRemoteError(error, "pairing").kind === "expired"
        ? { kind: "expired" } : { kind: "error", error, context: "pairing" });
    }
  };

  const deny = async () => {
    if (step.kind !== "review") return;
    const { request, revision: inspectedAt } = step;
    decided.current = true;
    setStep({ kind: "working", request, doing: "deny" });
    try {
      await denyRemotePairing(inspectedAt, request.pairingId);
      close();
    } catch (error) {
      decided.current = false;
      // A request that is already gone is as good as denied.
      if (describeRemoteError(error, "pairing").kind === "expired") close();
      else setStep({ kind: "error", error, context: "pairing" });
    }
  };

  const turnOn = async () => {
    setTurningOn(true);
    setTurnOnError(null);
    try {
      await remote.switchOn(space);
    } catch (error) {
      setTurnOnError(error);
    } finally {
      setTurningOn(false);
      void remote.refresh();
    }
  };

  const identity = step.kind === "review" || step.kind === "working" || step.kind === "approved"
    ? clientIdentity(step.request) : null;
  const name = identity
    ? (identity.kind === "known" ? identity.name : identity.host ?? t("remoteAccess.thisApp"))
    : "";
  const host = step.kind === "review" || step.kind === "working" || step.kind === "approved"
    ? step.request.redirectHost?.trim() || null : null;

  let title: string;
  let body: React.ReactNode = null;
  let actions: React.ReactNode;
  const closeButton = <Button variant="secondary" onClick={close}>{t("common.close")}</Button>;

  if (parsed.kind === "invalid") {
    title = t("remoteAccess.errorExpired");
    actions = closeButton;
  } else if (remote.nativeReadFailed) {
    title = t("remoteAccess.title");
    body = <RemoteErrorMessage error={remote.queryError} />;
    actions = closeButton;
  } else if (remote.nativeLoading) {
    title = t("remoteAccess.title");
    body = <p role="status" className="pairing-approval-body">{t("settings.controlState.loading")}</p>;
    actions = closeButton;
  } else if (remote.pendingDisconnect) {
    title = t("remoteAccess.title");
    body = <p role="status" className="pairing-approval-body">{t("remoteAccess.disconnectPending")}</p>;
    actions = closeButton;
  } else if (!profile?.enabled && status?.status !== "starting" && !connected) {
    // Web access is off. Turning it on is the one thing this dialog can do
    // before it knows who is asking.
    title = t("remoteAccess.offTitle");
    body = <>
      <p className="pairing-approval-body">{t("remoteAccess.offBody")}</p>
      <div className="pairing-approval-field">
        {remote.spaces.length === 0
          ? <p className="text-sm text-[var(--mem-text-secondary)]">{t("remoteAccess.noSpaces")}</p>
          : <Field label={t("remoteAccess.spaceToShare")} htmlFor={spaceId}>
              <Select value={space} disabled={turningOn} onChange={(event) => setChosen(event.target.value)}>
                {remote.spaces.map((item) => <option key={item.id} value={item.name}>{item.name}</option>)}
              </Select>
            </Field>}
      </div>
      {turnOnError !== null && <div className="pairing-approval-error"><RemoteErrorMessage error={turnOnError} /></div>}
    </>;
    actions = <>
      <Button variant="secondary" disabled={turningOn} onClick={close}>{t("remoteAccess.cancel")}</Button>
      <Button variant="primary" loading={turningOn} disabled={!space} onClick={() => { void turnOn(); }}>{t("remoteAccess.turnOn")}</Button>
    </>;
  } else if (!connected) {
    title = t("remoteAccess.title");
    body = status?.status === "error"
      ? <div className="pairing-approval-error"><RemoteErrorMessage error={status.error} /></div>
      : <p role="status" className="pairing-approval-body">{t("remoteAccess.statusConnecting")}</p>;
    actions = closeButton;
  } else if (step.kind === "loading") {
    title = t("remoteAccess.title");
    body = <p role="status" className="pairing-approval-body">{t("remoteAccess.statusConnecting")}</p>;
    actions = <Button variant="secondary" onClick={close}>{t("remoteAccess.cancel")}</Button>;
  } else if (step.kind === "expired") {
    title = t("remoteAccess.errorExpired");
    actions = closeButton;
  } else if (step.kind === "error") {
    title = t("remoteAccess.title");
    body = <div className="pairing-approval-error"><RemoteErrorMessage error={step.error} context={step.context} /></div>;
    actions = <>
      {closeButton}
      <Button variant="primary" onClick={() => { decided.current = false; setAttempt((value) => value + 1); }}>{t("remoteAccess.retry")}</Button>
    </>;
  } else if (step.kind === "approved") {
    title = t("remoteAccess.approvedTitle");
    body = <p role="status" className="pairing-approval-body">{t("remoteAccess.approvedBody", { name })}</p>;
    actions = <Button variant="primary" autoFocus onClick={close}>{t("remoteAccess.done")}</Button>;
  } else {
    // review, or a decision on its way
    const working = step.kind === "working";
    const unknown = identity?.kind === "unknown";
    title = t("remoteAccess.approveTitle", { name });
    body = <>
      <p className="pairing-approval-body">{t("remoteAccess.approveBody", { space: profile?.space ?? "" })}</p>
      {host && <p className="pairing-approval-sends">{t("remoteAccess.sendsTo", { host })}</p>}
      {unknown && <p role="note" className="pairing-approval-warning">{t("remoteAccess.unknownWarning")}</p>}
    </>;
    // An app Wenlan does not recognize gets the emphasis on saying no.
    actions = <>
      <Button variant={unknown ? "primary" : "secondary"} loading={working && step.doing === "deny"} disabled={working} onClick={() => { void deny(); }}>
        {t("remoteAccess.deny")}
      </Button>
      <Button variant={unknown ? "secondary" : "primary"} loading={working && step.doing === "allow"} disabled={working} onClick={() => { void allow(); }}>
        {working && step.doing === "allow" ? t("remoteAccess.approving") : t("remoteAccess.allow")}
      </Button>
    </>;
  }

  return (
    <div className="pairing-approval-backdrop">
      <div ref={panel} tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby={titleId}
        aria-describedby={body ? bodyId : undefined} className="pairing-approval-panel"
        onKeyDown={(event) => {
          if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); if (step.kind !== "working") close(); return; }
          if (event.key !== "Tab") return;
          const controls = Array.from(panel.current?.querySelectorAll<HTMLElement>("button:not(:disabled), select:not(:disabled), a[href]") ?? []);
          const first = controls[0];
          const last = controls[controls.length - 1];
          if (event.shiftKey && (document.activeElement === first || document.activeElement === panel.current)) {
            event.preventDefault(); last?.focus();
          } else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault(); first?.focus();
          }
        }}>
        <h2 id={titleId}>{title}</h2>
        {body && <div id={bodyId}>{body}</div>}
        <div className="pairing-approval-actions">{actions}</div>
      </div>
    </div>
  );
}
