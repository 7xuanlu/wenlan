// SPDX-License-Identifier: AGPL-3.0-only
import { useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { detectMcpClients, writeMcpConfig, type McpClient } from "../../lib/tauri";
import CliPrimaryPath, { isCliPrimaryClient, type CliClientType } from "./CliPrimaryPath";

/** Apps & CLIs group (spec §2a / §9.3). CLI clients lead with their primary
 *  plugin path (CliPrimaryPath: terminal commands + "Copy setup prompt");
 *  the one-click config write moves under Advanced. GUI clients keep the
 *  one-click "Set up". */
export default function ClientSetupList() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const { data: clients } = useQuery({ queryKey: ["mcp-clients"], queryFn: detectMcpClients });

  const setUp = async (clientType: string) => {
    setBusy(clientType);
    setErrors((prev) => ({ ...prev, [clientType]: "" }));
    try {
      await writeMcpConfig(clientType);
      queryClient.invalidateQueries({ queryKey: ["mcp-clients"] });
    } catch (err) {
      setErrors((prev) => ({ ...prev, [clientType]: String(err) }));
    } finally {
      setBusy(null);
    }
  };

  const rowShell = (client: McpClient, children: ReactNode) => (
    <div
      key={client.client_type}
      className="rounded-lg px-3 py-2.5 flex flex-col gap-2"
      style={{ border: "1px solid var(--mem-border)", backgroundColor: "var(--mem-surface)" }}
    >
      <div className="flex items-center gap-3">
        <div className="flex-1 min-w-0">
          <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", fontWeight: 500, color: "var(--mem-text)", margin: 0 }}>
            {client.name}
          </p>
          <p className="truncate" style={{ fontFamily: "var(--mem-font-mono)", fontSize: "10px", color: "var(--mem-text-tertiary)", margin: 0 }}>
            {client.config_path}
          </p>
        </div>
        {client.already_configured && (
          <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-accent-sage)" }}>
            {t("connectMatrix.configured")}
          </span>
        )}
      </div>
      {children}
      {errors[client.client_type] && (
        <p className="text-red-500" role="alert" style={{ fontFamily: "var(--mem-font-mono)", fontSize: "10px", margin: 0 }}>
          {errors[client.client_type]}
        </p>
      )}
    </div>
  );

  const advancedSetUp = (client: McpClient) => (
    <details>
      <summary style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)", cursor: "pointer" }}>
        {t("connectMatrix.advanced")}
      </summary>
      <button
        type="button"
        onClick={() => setUp(client.client_type)}
        disabled={busy === client.client_type}
        className="mt-2 rounded-md px-3 py-1.5 text-xs disabled:opacity-50"
        style={{ border: "1px solid var(--mem-border)", color: "var(--mem-text)", fontFamily: "var(--mem-font-body)" }}
      >
        {busy === client.client_type ? t("connectMatrix.settingUp") : t("connectMatrix.oneClickAdvanced")}
      </button>
    </details>
  );

  const guiPrimary = (client: McpClient) =>
    client.already_configured ? null : client.detected ? (
      <button
        type="button"
        onClick={() => setUp(client.client_type)}
        disabled={busy === client.client_type}
        className="self-start rounded-md px-3 py-1.5 text-xs disabled:opacity-50"
        style={{ backgroundColor: "var(--mem-accent-indigo)", color: "white", fontFamily: "var(--mem-font-body)" }}
      >
        {busy === client.client_type ? t("connectMatrix.settingUp") : t("connectMatrix.setUp")}
      </button>
    ) : (
      <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}>
        {t("connectMatrix.notDetected")}
      </span>
    );

  // A plugin-install command for a CLI that isn't on the machine can't
  // succeed anyway — gate the CLI primary path on detection too, same as
  // guiPrimary, so an undetected client always shows "Not detected".
  const cliPrimary = (client: McpClient, clientType: CliClientType) =>
    client.detected ? (
      <>
        <CliPrimaryPath clientType={clientType} />
        {advancedSetUp(client)}
      </>
    ) : (
      <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}>
        {t("connectMatrix.notDetected")}
      </span>
    );

  return (
    <div className="flex flex-col" style={{ gap: "8px" }}>
      {(clients ?? []).map((client) =>
        isCliPrimaryClient(client.client_type)
          ? rowShell(client, cliPrimary(client, client.client_type))
          : rowShell(client, guiPrimary(client)),
      )}
    </div>
  );
}
