// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { emit, listen } from "@tauri-apps/api/event";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { ArrowSquareOut, X } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import "./aboutWenlan.css";

interface AboutWenlanDialogProps {
  open: boolean;
  onClose: () => void;
}
interface UpdateStatus {
  state: "checking" | "current" | "available" | "error";
  version?: string;
}

export default function AboutWenlanDialog({ open, onClose }: AboutWenlanDialogProps) {
  const { t } = useTranslation();
  const panel = useRef<HTMLDivElement>(null);
  const [version, setVersion] = useState<string>();
  const [status, setStatus] = useState<UpdateStatus>();
  const [linkFailed, setLinkFailed] = useState(false);

  useEffect(() => {
    if (!open) return;
    let active = true;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panel.current?.focus();
    void getVersion().then((value) => { if (active) setVersion(value); }).catch(() => {});
    const unlisten = listen<UpdateStatus>("updater://status", ({ payload }) => {
      if (active) setStatus(payload);
    });
    void unlisten.then(() => { if (active) return emit("updater://ui-ready"); }).catch(() => {
      if (active) setStatus({ state: "error" });
    });
    return () => {
      active = false;
      void unlisten.then((stop) => stop()).catch(() => {});
      if (previous?.isConnected) previous.focus();
    };
  }, [open]);

  if (!open) return null;

  const check = async () => {
    setStatus({ state: "checking" });
    try { await emit("updater://check-now"); }
    catch { setStatus({ state: "error" }); }
  };
  const statusText = status?.state === "available"
    ? t("aboutWenlan.available", { version: status.version })
    : t(`aboutWenlan.${status?.state ?? "automatic"}`);
  const links = [
    ["releaseNotes", "https://github.com/7xuanlu/wenlan/releases"],
    ["website", "https://wenlan.app"],
    ["reportIssue", "https://github.com/7xuanlu/wenlan/issues"],
  ] as const;

  return (
    <div className="about-wenlan-backdrop" onClick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}>
      <div ref={panel} tabIndex={-1} role="dialog" aria-modal="true"
        aria-labelledby="about-wenlan-title" className="about-wenlan-panel"
        onKeyDown={(event) => {
          if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); onClose(); }
          if (event.key !== "Tab") return;
          const controls = Array.from(panel.current?.querySelectorAll<HTMLElement>('button:not(:disabled), a[href]') ?? []);
          const first = controls[0];
          const last = controls[controls.length - 1];
          if (event.shiftKey && (document.activeElement === first || document.activeElement === panel.current)) {
            event.preventDefault(); last?.focus();
          } else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault(); first?.focus();
          }
        }}>
        <button className="about-wenlan-close" type="button" aria-label={t("common.close")} onClick={onClose}><X size={18} /></button>
        <h2 id="about-wenlan-title">{t("aboutWenlan.title")}</h2>
        <p className="about-wenlan-description">{t("aboutWenlan.description")}</p>
        <p className="about-wenlan-version">{t("aboutWenlan.version", { version: version ?? "—" })}</p>
        <section className="about-wenlan-updates" aria-labelledby="about-updates-title">
          <h3 id="about-updates-title">{t("aboutWenlan.updates")}</h3>
          <p role="status">{statusText}</p>
          <button type="button" className="about-wenlan-check" disabled={status?.state === "checking"}
            onClick={() => void check()}>{t("aboutWenlan.check")}</button>
        </section>
        <nav className="about-wenlan-links" aria-label={t("aboutWenlan.resources")}>
          {links.map(([key, href]) => <a key={key} href={href} target="_blank" rel="noopener noreferrer" onClick={(event) => {
            event.preventDefault(); setLinkFailed(false);
            void openExternal(href).catch(() => setLinkFailed(true));
          }}>{t(`aboutWenlan.${key}`)}<ArrowSquareOut size={12} aria-hidden="true" /></a>)}
        </nav>
        {linkFailed && <p role="alert">{t("aboutWenlan.linkError")}</p>}
        <p className="about-wenlan-license">{t("aboutWenlan.license")}</p>
      </div>
    </div>
  );
}
