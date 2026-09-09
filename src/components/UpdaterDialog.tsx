// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";

interface ProgressPayload {
  chunk?: number;
  total?: number | null;
  done?: boolean;
  error?: string;
}

type UpdaterStatusState = "checking" | "current" | "available" | "error";

interface StatusPayload {
  state: UpdaterStatusState;
  version?: string;
  error?: string;
}

/**
 * In-app updater toast. Mirrors the MilestoneToaster visual language
 * (eyebrow + heading + body + mem-shadow-toast + mem-fade-up animation),
 * pinned to the main window's bottom-left so it travels with the window.
 *
 * Driven by Tauri events from `app/src/updater.rs`:
 *   - `updater://available`  payload `{ version }` → show toast
 *   - `updater://progress`   payload `{ chunk, total, error }`
 *   - `updater://status`     payload `{ state, version?, error? }`
 *   - `updater://ui-ready`   → ask the backend to replay a pending prompt
 * Sends back:
 *   - `updater://action`     payload `"install"` | `"later"`
 *   - `updater://check-now`   after a failed install to retry manually
 */
export default function UpdaterDialog() {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);
  const [version, setVersion] = useState("");
  const [installing, setInstalling] = useState(false);
  const [downloaded, setDownloaded] = useState(0);
  const [total, setTotal] = useState<number | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    const unlistenAvail = listen<{ version: string }>("updater://available", (e) => {
      setVersion(e.payload.version);
      setChecking(false);
      setInstalling(false);
      setDownloaded(0);
      setTotal(null);
      setErrorMsg(null);
      setVisible(true);
    });
    const unlistenProg = listen<ProgressPayload>("updater://progress", (e) => {
      const p = e.payload;
      if (p.error) {
        setErrorMsg(p.error);
        setInstalling(false);
        return;
      }
      if (p.chunk) setDownloaded((prev) => prev + (p.chunk ?? 0));
      if (p.total !== undefined && p.total !== null) setTotal(p.total);
    });
    const unlistenStatus = listen<StatusPayload>("updater://status", (e) => {
      const status = e.payload;
      setChecking(status.state === "checking");
      if (status.state === "checking") {
        setInstalling(true);
        setErrorMsg(null);
      } else if (status.state === "current") {
        setVisible(false);
        setInstalling(false);
        setErrorMsg(null);
      } else if (status.state === "error") {
        setInstalling(false);
        if (status.error) setErrorMsg(status.error);
      }
      if (status.version) setVersion(status.version);
    });
    void Promise.all([unlistenAvail, unlistenProg, unlistenStatus])
      .then(() => emit("updater://ui-ready"))
      .catch(() => {});
    return () => {
      unlistenAvail.then((fn) => fn());
      unlistenProg.then((fn) => fn());
      unlistenStatus.then((fn) => fn());
    };
  }, []);

  const handleInstall = async () => {
    if (installing || checking) return;
    setInstalling(true);
    try {
      await emit("updater://action", "install");
    } catch (error) {
      setInstalling(false);
      setErrorMsg(String(error));
    }
  };

  const handleLater = async () => {
    setVisible(false);
    try {
      await emit("updater://action", "later");
    } catch {
      setVisible(true);
    }
  };

  const handleRetry = async () => {
    if (checking) return;
    setChecking(true);
    setInstalling(true);
    setErrorMsg(null);
    try {
      await emit("updater://check-now");
    } catch (error) {
      setChecking(false);
      setInstalling(false);
      setErrorMsg(String(error));
    }
  };

  if (!visible) return null;

  const pct =
    total && total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null;

  const accent = "var(--mem-accent-indigo)";

  return (
    <div
      style={{
        position: "fixed",
        bottom: 24,
        left: 24,
        zIndex: 1000,
        fontFamily: "var(--mem-font-body)",
        color: "var(--mem-text)",
        backgroundColor: "var(--mem-surface)",
        border: "1px solid var(--mem-border)",
        borderRadius: 10,
        padding: "14px 18px",
        boxShadow: "var(--mem-shadow-toast)",
        animation: "mem-fade-up 400ms cubic-bezier(0.16, 1, 0.3, 1) both",
        maxWidth: 360,
        minWidth: 280,
      }}
    >
      <div
        style={{
          fontFamily: "var(--mem-font-mono)",
          fontSize: "10px",
          letterSpacing: "0.14em",
          textTransform: "uppercase",
          color: accent,
          marginBottom: 6,
          opacity: 0.85,
        }}
      >
        {errorMsg ? t("updater.failed") : installing ? t("updater.updating") : t("updater.available")}
      </div>

      <div
        style={{
          fontFamily: "var(--mem-font-heading)",
          fontSize: "15px",
          fontWeight: 500,
          lineHeight: 1.35,
          color: "var(--mem-text)",
          letterSpacing: "-0.005em",
        }}
      >
        {t("updater.ready", { version })}
      </div>

      <div
        style={{
          marginTop: 6,
          fontSize: "12.5px",
          lineHeight: 1.5,
          color: "var(--mem-text-secondary)",
        }}
      >
        {errorMsg
          ? errorMsg
          : installing
            ? pct !== null
              ? t("updater.downloading", { pct })
              : t("updater.preparing")
            : t("updater.installBody")}
      </div>

      {installing && !errorMsg && (
        <div
          style={{
            marginTop: 10,
            height: 3,
            background: "var(--mem-hover)",
            borderRadius: 2,
            overflow: "hidden",
          }}
        >
          <div
            style={{
              height: "100%",
              width: pct !== null ? `${pct}%` : "30%",
              background: accent,
              transition: "width 200ms linear",
              animation: pct === null ? "updater-pulse 1.5s ease-in-out infinite" : "none",
            }}
          />
        </div>
      )}

      {!installing && (
        <div style={{ display: "flex", gap: 6, justifyContent: "flex-end", marginTop: 12 }}>
          {!errorMsg && (
            <button
              onClick={handleLater}
              style={{
                padding: "5px 12px",
                borderRadius: 6,
                border: "1px solid var(--mem-border)",
                background: "transparent",
                color: "var(--mem-text)",
                fontSize: 12,
                cursor: "pointer",
                fontFamily: "var(--mem-font-body)",
              }}
            >
              {t("updater.later")}
            </button>
          )}
          <button
            onClick={errorMsg ? handleRetry : handleInstall}
            disabled={checking}
            style={{
              padding: "5px 12px",
              borderRadius: 6,
              border: "none",
              background: accent,
              color: "white",
              fontSize: 12,
              fontWeight: 500,
              cursor: checking ? "wait" : "pointer",
              fontFamily: "var(--mem-font-body)",
              opacity: checking ? 0.7 : 1,
            }}
          >
            {t("updater.install")}
          </button>
        </div>
      )}

      <style>{`
        @keyframes updater-pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.4; }
        }
      `}</style>
    </div>
  );
}
