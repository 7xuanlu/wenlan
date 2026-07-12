// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { clipboardWrite, getRemoteAccessStatus, listAgents } from "../../lib/tauri";

const CLAUDE_CONNECTOR_URL = "https://claude.ai/settings/connectors?modal=add-custom-connector";

/** Per-platform web connect cards (spec §2a group 1; §9.3 round 2: the
 *  Claude card is plugin-first — step 1 installs the Wenlan plugin via the
 *  Directory's Add-marketplace dialog, step 2 wires memory access via the
 *  custom connector). Verification is the existing listAgents delta poll —
 *  best-effort attribution: a new agent in the poll window flips the
 *  last-copied card (hint, not proof). */
export default function WebPlatformCards() {
  const { t } = useTranslation();
  const [copiedPlatform, setCopiedPlatform] = useState<string | null>(null);
  const [connectedPlatform, setConnectedPlatform] = useState<string | null>(null);
  const baselineCount = useRef<number | null>(null);

  const { data: remote } = useQuery({
    queryKey: ["remote-access-status"],
    queryFn: getRemoteAccessStatus,
    refetchInterval: 3000,
  });
  const url =
    remote?.status === "connected" ? (remote.relay_url ?? `${remote.tunnel_url}/mcp`) : null;

  const { data: agents } = useQuery({
    queryKey: ["web-connect-agents"],
    queryFn: listAgents,
    refetchInterval: copiedPlatform !== null && connectedPlatform === null ? 3000 : false,
  });
  useEffect(() => {
    if (!agents) return;
    if (baselineCount.current === null) {
      baselineCount.current = agents.length;
      return;
    }
    if (copiedPlatform !== null && agents.length > baselineCount.current) {
      setConnectedPlatform(copiedPlatform);
    }
  }, [agents, copiedPlatform]);

  const copy = async (platform: string) => {
    if (!url) return;
    try {
      await clipboardWrite(url);
    } catch {
      return;
    }
    baselineCount.current = agents?.length ?? baselineCount.current;
    setCopiedPlatform(platform);
  };

  const stepHeading = (text: string) => (
    <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", fontWeight: 600, color: "var(--mem-text)", margin: 0 }}>
      {text}
    </p>
  );

  const stepList = (steps: string[]) => (
    <ol style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", lineHeight: 1.7, paddingLeft: "18px", listStyle: "decimal", margin: 0 }}>
      {steps.map((s) => (
        <li key={s}>{s}</li>
      ))}
    </ol>
  );

  const urlRow = (platform: string) =>
    url ? (
      <div className="flex items-center gap-2">
        <code
          className="flex-1 truncate rounded-md px-2 py-1.5"
          style={{ fontFamily: "var(--mem-font-mono)", fontSize: "11px", backgroundColor: "var(--mem-bg)", border: "1px solid var(--mem-border)", color: "var(--mem-text)" }}
        >
          {url}
        </code>
        <button
          onClick={() => copy(platform)}
          className="rounded-md px-3 py-1.5 text-xs shrink-0"
          style={{ border: "1px solid var(--mem-border)", color: "var(--mem-text)", fontFamily: "var(--mem-font-body)" }}
        >
          {copiedPlatform === platform ? t("connectMatrix.copied") : t("connectMatrix.copyUrl")}
        </button>
      </div>
    ) : (
      <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-tertiary)" }}>
        {t("connectMatrix.tunnelOff")}
      </p>
    );

  /* No-auth boundary (council change f, commit 3a272d0): always visible. */
  const noAuthWarning = (
    <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-accent-amber)", lineHeight: 1.5 }}>
      {t("connectMatrix.noAuthWarning")}
    </p>
  );

  const cardShell = (platform: "claude" | "chatgpt", title: string, children: ReactNode) => (
    <div
      className="rounded-xl p-4 flex flex-col"
      style={{ border: "1px solid var(--mem-border)", backgroundColor: "var(--mem-surface)", gap: "10px" }}
    >
      <div className="flex items-center justify-between">
        <h3 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "15px", fontWeight: 500, color: "var(--mem-text)" }}>
          {title}
        </h3>
        {connectedPlatform === platform && (
          <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-accent-sage)" }}>
            {t("connectMatrix.connectedHint")}
          </span>
        )}
      </div>
      {children}
    </div>
  );

  return (
    <div className="flex flex-col" style={{ gap: "12px" }}>
      {cardShell(
        "claude",
        t("connectMatrix.claudeTitle"),
        <>
          {/* Step 1 — install the Wenlan plugin (§9.3: actionable today via
              Directory → Plugins → + Add marketplace, Personal tab). */}
          {stepHeading(t("connectMatrix.claudePluginStepTitle"))}
          {stepList([
            t("connectMatrix.claudePluginStep1"),
            t("connectMatrix.claudePluginStep2"),
            t("connectMatrix.claudePluginStep3"),
          ])}
          <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)", lineHeight: 1.5, margin: 0 }}>
            {t("connectMatrix.claudePluginNote")}
          </p>
          {/* Step 2 — memory access: a public plugin cannot carry a
              user-specific tunnel URL, so chat still needs the connector. */}
          {stepHeading(t("connectMatrix.claudeConnectorStepTitle"))}
          {stepList([
            t("connectMatrix.claudeStep1"),
            t("connectMatrix.claudeStep2"),
            t("connectMatrix.claudeStep3"),
          ])}
          {urlRow("claude")}
          {url && (
            <button
              type="button"
              onClick={() => shellOpen(CLAUDE_CONNECTOR_URL)}
              className="self-start rounded-md px-3 py-1.5 text-xs"
              style={{ border: "1px solid var(--mem-border)", color: "var(--mem-text)", fontFamily: "var(--mem-font-body)" }}
            >
              {t("connectMatrix.openConnectorSettings")}
            </button>
          )}
          {noAuthWarning}
        </>,
      )}
      {cardShell(
        "chatgpt",
        t("connectMatrix.chatgptTitle"),
        <>
          {stepList([
            t("connectMatrix.chatgptStep1"),
            t("connectMatrix.chatgptStep2"),
            t("connectMatrix.chatgptStep3"),
          ])}
          {urlRow("chatgpt")}
          {noAuthWarning}
        </>,
      )}
    </div>
  );
}
