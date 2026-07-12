// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  addSource,
  syncRegisteredSource,
  listRegisteredSources,
  type RegisteredSource,
} from "../../../lib/tauri";
import { detectVault, type VaultDetection } from "../../../lib/vaultDetection";

interface Props {
  variant: "dialog" | "wizard";
  onConnected?: (source: RegisteredSource) => void;
}

export default function VaultConnectCard({ variant, onConnected }: Props) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [path, setPath] = useState("");
  const [detection, setDetection] = useState<VaultDetection | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [connectedId, setConnectedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Post-connect: poll the registered source until it reports counts
  // ("Indexed N files · M memories", spec §3). Wizard variant only — the
  // dialog closes into the sources list which already polls.
  const { data: connectedSource } = useQuery({
    queryKey: ["vault-connect-progress", connectedId],
    queryFn: async () => {
      const sources = await listRegisteredSources();
      return sources.find((s) => s.id === connectedId) ?? null;
    },
    enabled: variant === "wizard" && connectedId !== null,
    refetchInterval: 2000,
  });

  const handleBrowse = useCallback(async () => {
    const selected = await openDialog({ directory: true, multiple: false });
    if (!selected || typeof selected !== "string") return;
    setPath(selected);
    setError(null);
    setDetection(null);
    setDetecting(true);
    setDetection(await detectVault(selected));
    setDetecting(false);
  }, []);

  const handleConnect = useCallback(async () => {
    setError(null);
    setConnecting(true);
    try {
      const source = await addSource(detection?.sourceType ?? "directory", path);
      queryClient.invalidateQueries({ queryKey: ["registeredSources"] });
      // Obsidian vaults are not on the daemon's 30s directory scheduler —
      // kick a one-shot first index (same rationale as AddSourceDialog).
      if (source.source_type === "obsidian") {
        syncRegisteredSource(source.id).then(() => {
          queryClient.invalidateQueries({ queryKey: ["registeredSources"] });
          queryClient.invalidateQueries({ queryKey: ["vault-connect-progress", source.id] });
        });
      }
      setConnectedId(source.id);
      onConnected?.(source);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setConnecting(false);
    }
  }, [detection, path, queryClient, onConnected]);

  // Submit is never blocked by a zero count (council change e): the daemon's
  // POST /api/sources validation is the authority; its 4xx surfaces verbatim.
  const canSubmit = path.length > 0 && !detecting && !connecting && connectedId === null;

  return (
    <div
      className="rounded-xl p-4 flex flex-col"
      style={{ border: "1px solid var(--mem-border)", backgroundColor: "var(--mem-surface)", gap: "12px" }}
    >
      <div>
        <h3 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "15px", fontWeight: 500, color: "var(--mem-text)" }}>
          {t("vaultConnect.title")}
        </h3>
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", lineHeight: 1.5, marginTop: "4px" }}>
          {t("vaultConnect.description")}
        </p>
      </div>

      <div className="flex gap-2">
        <input
          type="text"
          value={path}
          readOnly
          placeholder={t("vaultConnect.placeholder")}
          className="flex-1 rounded-md px-3 py-2 text-sm"
          style={{
            border: "1px solid var(--mem-border)",
            backgroundColor: "var(--mem-bg)",
            color: "var(--mem-text)",
            fontFamily: "var(--mem-font-mono)",
            fontSize: "12px",
          }}
        />
        <button
          onClick={handleBrowse}
          className="rounded-md px-3 py-2 text-sm"
          style={{ border: "1px solid var(--mem-border)", color: "var(--mem-text)", fontFamily: "var(--mem-font-body)" }}
        >
          {t("vaultConnect.browse")}
        </button>
      </div>

      {detecting && (
        <p style={{ fontSize: "12px", color: "var(--mem-text-secondary)", fontFamily: "var(--mem-font-body)" }}>
          {t("vaultConnect.scanning")}
        </p>
      )}

      {detection && !detecting && (
        <div style={{ fontSize: "12px", fontFamily: "var(--mem-font-body)", display: "flex", flexDirection: "column", gap: "2px" }}>
          {detection.isVault && (
            <span style={{ color: "var(--mem-accent-indigo)" }}>{t("vaultConnect.detectedVault")}</span>
          )}
          {detection.docCount > 0 ? (
            <span style={{ color: "var(--mem-text-secondary)" }}>
              {detection.countCapped
                ? t("vaultConnect.filesFoundCapped")
                : t("vaultConnect.filesFound", { count: detection.docCount })}
            </span>
          ) : (
            <span style={{ color: "var(--mem-accent-amber)" }}>{t("vaultConnect.noneFound")}</span>
          )}
          {detection.isVault && !detection.hasValidDoc && (
            <span style={{ color: "var(--mem-text-tertiary)" }}>{t("vaultConnect.vaultMarkdownOnly")}</span>
          )}
        </div>
      )}

      {error && <p className="text-red-500" style={{ fontSize: "12px", fontFamily: "var(--mem-font-mono)" }}>{error}</p>}

      {connectedId === null ? (
        <button
          onClick={handleConnect}
          disabled={!canSubmit}
          className="self-end rounded-md px-4 py-2 text-sm font-medium disabled:opacity-50"
          style={{ backgroundColor: "var(--mem-accent-indigo)", color: "white", fontFamily: "var(--mem-font-body)" }}
        >
          {connecting ? t("vaultConnect.connecting") : t("vaultConnect.connect")}
        </button>
      ) : (
        <p style={{ fontSize: "12px", color: "var(--mem-text-secondary)", fontFamily: "var(--mem-font-body)" }}>
          {connectedSource && connectedSource.file_count > 0
            ? t("vaultConnect.indexed", {
                files: connectedSource.file_count,
                memories: connectedSource.memory_count,
              })
            : t("vaultConnect.indexing")}
        </p>
      )}
    </div>
  );
}
