import { useState, useEffect, useRef, useMemo } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Trans, useTranslation } from "react-i18next";
import {
  detectMcpClients,
  writeMcpConfig,
  listAgents,
  getWenlanMcpEntry,
  type ImportResult,
} from "../lib/tauri";
import { ImportView } from "./memory/ImportView";
import VaultConnectCard from "./memory/sources/VaultConnectCard";
import { RemoteAccessPanel } from "./memory/RemoteAccessPanel";
import WebPlatformCards from "./connect/WebPlatformCards";
import CliPrimaryPath, { isCliPrimaryClient } from "./connect/CliPrimaryPath";
import { ApiKeyCard, OnDeviceModelCard } from "./intelligence/IntelligenceSetup";
import AnyProviderCard from "./intelligence/AnyProviderCard";
import { PROVIDER_PRESETS, ANTHROPIC_VENDOR_NAME } from "./intelligence/providerPresets";

export type WizardStep = "welcome" | "intelligence-choice" | "import" | "connect" | "verify" | "done";

interface SetupWizardProps {
  onComplete: () => void;
  initialStep?: WizardStep;
}

const STEP_ORDER: WizardStep[] = [
  "welcome",
  "intelligence-choice",
  "import",
  "connect",
  "verify",
  "done",
];

const MEMORY_TYPE_COLORS: Record<string, string> = {
  identity: "var(--mem-accent-indigo)",
  preference: "var(--mem-accent-sage)",
  fact: "var(--mem-accent-amber)",
  decision: "#a78bfa",
  goal: "#38bdf8",
};

// ── Step Indicator ──────────────────────────────────────────────────────

function StepIndicator({ currentStep }: { currentStep: WizardStep }) {
  const currentIndex = STEP_ORDER.indexOf(currentStep);

  return (
    <div
      className="flex items-center justify-center gap-2"
      style={{ padding: "16px 0" }}
    >
      {STEP_ORDER.map((step, i) => (
        <div
          key={step}
          style={{
            width: "8px",
            height: "8px",
            borderRadius: "50%",
            backgroundColor:
              i <= currentIndex
                ? "var(--mem-accent-indigo)"
                : "var(--mem-hover-strong)",
            transition: "background-color 0.3s ease",
          }}
        />
      ))}
    </div>
  );
}

// ── Welcome Step ────────────────────────────────────────────────────────

function WelcomeStep({ onNext }: { onNext: () => void }) {
  const { t } = useTranslation();

  return (
    <div
      className="flex flex-col items-center text-center"
      style={{ gap: "32px", paddingTop: "80px" }}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
        <h1
          style={{
            fontFamily: "var(--mem-font-heading)",
            fontSize: "28px",
            fontWeight: 500,
            color: "var(--mem-text)",
            letterSpacing: "-0.02em",
          }}
        >
          {t("setup.welcomeTitle")}
        </h1>
        <p
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "15px",
            color: "var(--mem-text-secondary)",
            lineHeight: "1.5",
          }}
        >
          {t("setup.tagline")}
        </p>
      </div>

      {/* Privacy badge */}
      <div
        className="flex items-center gap-2"
        style={{
          padding: "10px 16px",
          borderRadius: "10px",
          backgroundColor: "var(--mem-surface)",
          border: "1px solid var(--mem-border)",
        }}
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="var(--mem-text-secondary)"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0110 0v4" />
        </svg>
        <span
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "13px",
            color: "var(--mem-text-secondary)",
          }}
        >
          {t("setup.privacyTitle")}
        </span>
      </div>

      <button
        onClick={onNext}
        className="px-6 py-2.5 rounded-lg text-sm font-medium transition-colors duration-150"
        style={{
          fontFamily: "var(--mem-font-body)",
          backgroundColor: "var(--mem-accent-indigo)",
          color: "white",
          fontSize: "14px",
        }}
      >
        {t("setup.getStarted")}
      </button>
    </div>
  );
}

function IntelligenceChoiceStep({
  onNext,
  onBack,
}: {
  onNext: () => void;
  onBack: () => void;
}) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<"device" | "cloud" | "local">("device");
  // Cloud vendor pills: Anthropic (native slot) first, then cloud presets
  // (external slot) — spec §2.
  const [cloudVendor, setCloudVendor] = useState<string>("anthropic");
  const cloudPresets = PROVIDER_PRESETS.filter((p) => p.group === "cloud");

  const choiceButtonStyle = (active: boolean): React.CSSProperties => ({
    flex: 1,
    padding: "10px 12px",
    borderRadius: "10px",
    border: "1px solid var(--mem-border)",
    backgroundColor: active ? "rgba(99, 102, 241, 0.12)" : "var(--mem-surface)",
    color: active ? "var(--mem-accent-indigo)" : "var(--mem-text-secondary)",
    fontFamily: "var(--mem-font-body)",
    fontSize: "13px",
    fontWeight: 500,
    textAlign: "left",
  });

  const pillStyle = (active: boolean): React.CSSProperties => ({
    padding: "6px 12px",
    borderRadius: "999px",
    border: "1px solid var(--mem-border)",
    backgroundColor: active ? "rgba(99, 102, 241, 0.12)" : "var(--mem-surface)",
    color: active ? "var(--mem-accent-indigo)" : "var(--mem-text-secondary)",
    fontFamily: "var(--mem-font-body)",
    fontSize: "12px",
    fontWeight: 500,
  });

  const note = (
    key:
      | "setup.intelligence.deviceNote"
      | "setup.intelligence.cloudNote"
      | "setup.intelligence.localNote",
  ) => (
    <div
      className="rounded-xl px-4 py-3"
      style={{
        backgroundColor: "var(--mem-hover)",
        border: "1px solid var(--mem-border)",
        fontFamily: "var(--mem-font-body)",
        fontSize: "12px",
        color: "var(--mem-text-secondary)",
        lineHeight: 1.6,
      }}
    >
      {t(key)}
    </div>
  );

  return (
    <div
      className="flex flex-col max-w-xl mx-auto"
      style={{ gap: "24px", paddingTop: "24px" }}
    >
      <button
        onClick={onBack}
        className="flex items-center gap-1.5 self-start transition-colors duration-150"
        style={{
          fontFamily: "var(--mem-font-body)",
          fontSize: "13px",
          color: "var(--mem-text-secondary)",
        }}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M19 12H5M12 19l-7-7 7-7" />
        </svg>
        {t("setup.back")}
      </button>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <h1
          style={{
            fontFamily: "var(--mem-font-heading)",
            fontSize: "20px",
            fontWeight: 500,
            color: "var(--mem-text)",
          }}
        >
          {t("setup.intelligence.title")}
        </h1>
        <p
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "13px",
            color: "var(--mem-text-secondary)",
            lineHeight: "1.5",
          }}
        >
          {t("setup.intelligence.description")}
        </p>
      </div>

      <div className="flex gap-3">
        <button onClick={() => setMode("device")} style={choiceButtonStyle(mode === "device")}>
          {t("setup.intelligence.deviceOption")}
        </button>
        <button onClick={() => setMode("cloud")} style={choiceButtonStyle(mode === "cloud")}>
          {t("setup.intelligence.cloudOption")}
        </button>
        <button onClick={() => setMode("local")} style={choiceButtonStyle(mode === "local")}>
          {t("setup.intelligence.localOption")}
        </button>
      </div>

      {mode === "device" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
          <OnDeviceModelCard />
          {note("setup.intelligence.deviceNote")}
        </div>
      )}

      {mode === "cloud" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
          <div className="flex flex-wrap gap-2">
            <button onClick={() => setCloudVendor("anthropic")} style={pillStyle(cloudVendor === "anthropic")}>
              {ANTHROPIC_VENDOR_NAME}
            </button>
            {cloudPresets.map((p) => (
              <button key={p.id} onClick={() => setCloudVendor(p.id)} style={pillStyle(cloudVendor === p.id)}>
                {p.name}
              </button>
            ))}
          </div>
          {cloudVendor === "anthropic" ? (
            <ApiKeyCard showNoKeyGuidance={false} />
          ) : (
            <AnyProviderCard
              key={cloudVendor}
              groups={["cloud"]}
              initialPresetId={cloudVendor}
              hidePresetPicker
            />
          )}
          {note("setup.intelligence.cloudNote")}
        </div>
      )}

      {mode === "local" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "12px" }}>
          <AnyProviderCard groups={["local"]} />
          {note("setup.intelligence.localNote")}
        </div>
      )}

      <div
        className="flex items-center gap-3"
        style={{
          paddingTop: "16px",
          borderTop: "1px solid var(--mem-border)",
        }}
      >
        <button
          onClick={onNext}
          className="ml-auto transition-colors duration-150"
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "13px",
            color: "var(--mem-text-tertiary)",
            background: "none",
            border: "none",
            cursor: "pointer",
          }}
        >
          {t("setup.skip")}
        </button>
        <button
          onClick={onNext}
          className="px-5 py-2 rounded-lg text-sm font-medium transition-colors duration-150"
          style={{
            fontFamily: "var(--mem-font-body)",
            backgroundColor: "var(--mem-accent-indigo)",
            color: "white",
          }}
        >
          {t("setup.continue")}
        </button>
      </div>
    </div>
  );
}

// ── Import Step (dual path: chat history | vault) ───────────────────────

function ImportStep({
  onBack,
  onSkip,
  onComplete,
  onPhaseChange,
  importHint,
}: {
  onBack: () => void;
  onSkip: () => void;
  onComplete: (source: string, result: ImportResult) => void;
  onPhaseChange: (phase: string) => void;
  importHint: React.ReactNode;
}) {
  const { t } = useTranslation();
  const [pathChoice, setPathChoice] = useState<"none" | "chat">("none");

  if (pathChoice === "chat") {
    return (
      <ImportView
        onBack={() => setPathChoice("none")}
        wizardMode
        wizardHint={importHint}
        onPhaseChange={onPhaseChange}
        onSkip={onSkip}
        onComplete={onComplete}
      />
    );
  }

  return (
    <div className="flex flex-col max-w-xl mx-auto" style={{ gap: "24px", paddingTop: "24px" }}>
      <button
        onClick={onBack}
        className="flex items-center gap-1.5 self-start transition-colors duration-150"
        style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", color: "var(--mem-text-secondary)" }}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M19 12H5M12 19l-7-7 7-7" />
        </svg>
        {t("setup.back")}
      </button>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <h1 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "20px", fontWeight: 500, color: "var(--mem-text)" }}>
          {t("setup.import.title")}
        </h1>
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", color: "var(--mem-text-secondary)", lineHeight: "1.5" }}>
          {t("setup.import.description")}
        </p>
      </div>

      <div
        className="rounded-xl p-4 flex items-center justify-between"
        style={{ border: "1px solid var(--mem-border)", backgroundColor: "var(--mem-surface)", gap: "12px" }}
      >
        <div>
          <h3 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "15px", fontWeight: 500, color: "var(--mem-text)" }}>
            {t("setup.import.chatPathTitle")}
          </h3>
          <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", marginTop: "4px" }}>
            {t("setup.import.chatPathDescription")}
          </p>
        </div>
        <button
          onClick={() => setPathChoice("chat")}
          className="rounded-md px-4 py-2 text-sm font-medium shrink-0"
          style={{ backgroundColor: "var(--mem-accent-indigo)", color: "white", fontFamily: "var(--mem-font-body)" }}
        >
          {t("setup.import.chatPathCta")}
        </button>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <h3 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "15px", fontWeight: 500, color: "var(--mem-text)" }}>
          {t("setup.import.vaultPathTitle")}
        </h3>
        <VaultConnectCard variant="wizard" />
      </div>

      <div className="flex items-center" style={{ paddingTop: "16px", borderTop: "1px solid var(--mem-border)" }}>
        <button
          onClick={onSkip}
          className="ml-auto transition-colors duration-150"
          style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", color: "var(--mem-text-tertiary)", background: "none", border: "none", cursor: "pointer" }}
        >
          {t("setup.skip")}
        </button>
        <button
          onClick={onSkip}
          className="px-5 py-2 rounded-lg text-sm font-medium transition-colors duration-150 ml-3"
          style={{ fontFamily: "var(--mem-font-body)", backgroundColor: "var(--mem-accent-indigo)", color: "white" }}
        >
          {t("setup.continue")}
        </button>
      </div>
    </div>
  );
}

// ── Connect Step ────────────────────────────────────────────────────────

/** Small uppercase sub-section label used inside ConnectStep to group
 *  "Detected on your Mac" / "Claude.ai & ChatGPT (web)" / "Advanced". */
function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <p
      style={{
        fontFamily: "var(--mem-font-body)",
        fontSize: "11px",
        fontWeight: 600,
        color: "var(--mem-text-tertiary)",
        letterSpacing: "0.06em",
        textTransform: "uppercase",
        margin: 0,
      }}
    >
      {children}
    </p>
  );
}

function ConnectStep({
  onNext,
  onBack,
  onConnected,
}: {
  onNext: () => void;
  onBack: () => void;
  onConnected: (agents: string[]) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [connectedClients, setConnectedClients] = useState<Record<string, boolean>>({});
  const [connectErrors, setConnectErrors] = useState<Record<string, string>>({});
  const [manualExpanded, setManualExpanded] = useState(false);
  const [selectedClients, setSelectedClients] = useState<Record<string, boolean>>({});
  const [isConnectingAll, setIsConnectingAll] = useState(false);

  const { data: clients, isLoading, isError } = useQuery({
    queryKey: ["mcp-clients"],
    queryFn: detectMcpClients,
  });

  useEffect(() => {
    if (!clients) return;
    setSelectedClients((prev) => {
      const next = { ...prev };
      for (const client of clients) {
        if (next[client.client_type] === undefined) {
          // §9.3: CLI clients lead with the plugin path; the one-click batch
          // write is opt-in for them, so their checkbox starts unchecked.
          next[client.client_type] =
            client.detected &&
            !client.already_configured &&
            !isCliPrimaryClient(client.client_type);
        }
      }
      return next;
    });
  }, [clients]);

  useEffect(() => {
    if (!clients) return;
    const configuredNames = clients
      .filter((client) => client.already_configured)
      .map((client) => client.name);
    if (configuredNames.length > 0) {
      onConnected(configuredNames);
    }
  }, [clients, onConnected]);

  useEffect(() => {
    if (isError) {
      setManualExpanded(true);
    }
  }, [isError]);

  const { data: wenlanMcpEntry } = useQuery({
    queryKey: ["wenlan-mcp-entry"],
    queryFn: getWenlanMcpEntry,
    staleTime: Infinity,
  });

  const mcpJsonSnippet = `{
  "mcpServers": {
    "wenlan": ${JSON.stringify(wenlanMcpEntry ?? { command: "npx", args: ["-y", "wenlan-mcp"] }, null, 6).replace(/\n/g, "\n    ")}
  }
}`;

  const handleContinue = async () => {
    if (!clients) {
      onNext();
      return;
    }

    const toConnect = clients.filter(
      (client) =>
        selectedClients[client.client_type] &&
        client.detected &&
        !client.already_configured &&
        !connectedClients[client.client_type],
    );

    if (toConnect.length === 0) {
      onNext();
      return;
    }

    setIsConnectingAll(true);
    const newlyConnected: string[] = [];
    let hadErrors = false;

    for (const client of toConnect) {
      try {
        await writeMcpConfig(client.client_type);
        setConnectedClients((prev) => ({ ...prev, [client.client_type]: true }));
        setConnectErrors((prev) => {
          const next = { ...prev };
          delete next[client.client_type];
          return next;
        });
        newlyConnected.push(client.name);
      } catch (err) {
        hadErrors = true;
        setConnectErrors((prev) => ({
          ...prev,
          [client.client_type]: String(err),
        }));
      }
    }

    if (newlyConnected.length > 0) {
      onConnected(newlyConnected);
      queryClient.invalidateQueries({ queryKey: ["mcp-clients"] });
    }

    setIsConnectingAll(false);
    if (!hadErrors) {
      onNext();
    }
  };

  const detectedClients = (clients ?? []).filter((client) => client.detected);
  const supportedClients = (clients ?? []).filter((client) => !client.detected);

  const renderClientList = (
    list: NonNullable<typeof clients>,
    statusLabel: string,
    statusActive: boolean,
  ) => (
    <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
      {list.map((client) => {
        const isConnected = connectedClients[client.client_type] || client.already_configured;
        const error = connectErrors[client.client_type];
        const isSelected = !!selectedClients[client.client_type];

        // §9.3: detected, not-yet-connected CLI clients lead with the shared
        // plugin path instead of the generic description.
        const isCliPrimary =
          client.detected && !isConnected && isCliPrimaryClient(client.client_type);
        const descId = `client-desc-${client.client_type}`;

        const nameBadges = (
          <div className="flex items-center gap-2 flex-wrap">
            <span
              style={{
                fontFamily: "var(--mem-font-body)",
                fontSize: "14px",
                fontWeight: 500,
                color: "var(--mem-text)",
              }}
            >
              {client.name}
            </span>
            <span
              className="px-2 py-0.5 rounded-full text-xs"
              style={{
                fontFamily: "var(--mem-font-body)",
                backgroundColor: statusActive ? "rgba(99, 102, 241, 0.12)" : "var(--mem-hover)",
                color: statusActive ? "var(--mem-accent-indigo)" : "var(--mem-text-tertiary)",
              }}
            >
              {statusLabel}
            </span>
            {isConnected && (
              <span
                className="px-2 py-0.5 rounded-full text-xs"
                style={{
                  fontFamily: "var(--mem-font-body)",
                  backgroundColor: "rgba(34,197,94,0.12)",
                  color: "rgb(34,197,94)",
                }}
              >
                {t("setup.connect.configured")}
              </span>
            )}
          </div>
        );

        const checkbox = (
          <input
            type="checkbox"
            aria-label={client.name}
            aria-describedby={isCliPrimary ? descId : undefined}
            checked={isSelected || isConnected}
            disabled={!client.detected || isConnected || isConnectingAll}
            onChange={(e) => setSelectedClients((prev) => ({ ...prev, [client.client_type]: e.target.checked }))}
            style={{
              width: "16px",
              height: "16px",
              accentColor: "var(--mem-accent-indigo)",
              marginTop: "2px",
            }}
          />
        );

        // The CLI primary path renders a real <button> ("Copy setup
        // prompt"). Nesting that inside the same <label> that toggles this
        // row's checkbox risks the browser forwarding the click to the
        // checkbox too, so hint/status content (commands, reload note,
        // error) lives in a sibling node wired via aria-describedby instead
        // of inside the <label> — the <label> only wraps the checkbox and
        // its plain-text name/status badges.
        //
        // The row is stacked (flex-col), not side-by-side: the wizard's
        // connect column is max-w-md, so a flex-row split left CliPrimaryPath
        // with ~200px and its command line (a truncating <code>) unreadable.
        // The description div gets left padding equal to checkbox width
        // (16px) + the label's gap-3 (12px) so it aligns under the client
        // name instead of under the checkbox.
        if (isCliPrimary && isCliPrimaryClient(client.client_type)) {
          return (
            <div
              key={client.client_type}
              className="flex flex-col gap-2 rounded-xl px-4 py-3"
              style={{
                backgroundColor: "var(--mem-surface)",
                border: `1px solid ${isSelected ? "rgba(99, 102, 241, 0.4)" : "var(--mem-border)"}`,
              }}
            >
              <label className="flex items-start gap-3" style={{ cursor: "pointer" }}>
                {checkbox}
                {nameBadges}
              </label>

              <div
                id={descId}
                style={{ display: "flex", flexDirection: "column", gap: "6px", paddingLeft: "28px" }}
              >
                <CliPrimaryPath clientType={client.client_type} />
                <p
                  style={{
                    fontFamily: "var(--mem-font-body)",
                    fontSize: "11px",
                    color: "var(--mem-text-tertiary)",
                    lineHeight: 1.5,
                    margin: 0,
                  }}
                >
                  {t("connectMatrix.oneClickAdvanced")}
                </p>

                {error && (
                  <p
                    style={{
                      fontFamily: "var(--mem-font-body)",
                      fontSize: "11px",
                      color: "#ef4444",
                      margin: 0,
                    }}
                  >
                    {error}
                  </p>
                )}
              </div>
            </div>
          );
        }

        return (
          <label
            key={client.client_type}
            className="flex items-start gap-3 rounded-xl px-4 py-3"
            style={{
              backgroundColor: "var(--mem-surface)",
              border: `1px solid ${isSelected ? "rgba(99, 102, 241, 0.4)" : "var(--mem-border)"}`,
              cursor: client.detected && !isConnected ? "pointer" : "default",
            }}
          >
            {checkbox}

            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: "6px" }}>
              {nameBadges}

              <p
                style={{
                  fontFamily: "var(--mem-font-body)",
                  fontSize: "12px",
                  color: "var(--mem-text-secondary)",
                  lineHeight: 1.5,
                  margin: 0,
                }}
              >
                {client.detected
                  ? t("setup.connect.detectedDescription")
                  : t("setup.connect.supportedDescription")}
              </p>

              {error && (
                <p
                  style={{
                    fontFamily: "var(--mem-font-body)",
                    fontSize: "11px",
                    color: "#ef4444",
                    margin: 0,
                  }}
                >
                  {error}
                </p>
              )}
            </div>
          </label>
        );
      })}
    </div>
  );

  return (
    <div
      className="flex flex-col max-w-md mx-auto"
      style={{ gap: "24px", paddingTop: "24px" }}
    >
      <button
        onClick={onBack}
        className="flex items-center gap-1.5 self-start transition-colors duration-150"
        style={{
          fontFamily: "var(--mem-font-body)",
          fontSize: "13px",
          color: "var(--mem-text-secondary)",
        }}
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M19 12H5M12 19l-7-7 7-7" />
        </svg>
        {t("setup.back")}
      </button>

      <div>
        <h1
          style={{
            fontFamily: "var(--mem-font-heading)",
            fontSize: "20px",
            fontWeight: 500,
            color: "var(--mem-text)",
          }}
        >
          {t("setup.connect.title")}
        </h1>
        <p
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "13px",
            color: "var(--mem-text-secondary)",
            marginTop: "4px",
            lineHeight: "1.5",
          }}
        >
          {t("setup.connect.description")}
        </p>
      </div>

      {(isLoading || isError || detectedClients.length > 0) && (
        <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
          <SectionLabel>{t("setup.connect.detectedOnMac")}</SectionLabel>

          {isLoading && (
            <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
              {[0, 1].map((i) => (
                <div
                  key={i}
                  className="rounded-xl"
                  style={{
                    height: "72px",
                    backgroundColor: "var(--mem-surface)",
                    border: "1px solid var(--mem-border)",
                    opacity: 0.5,
                    animation: "pulse 1.6s ease-in-out infinite",
                  }}
                />
              ))}
            </div>
          )}

          {isError && (
            <p
              style={{
                fontFamily: "var(--mem-font-body)",
                fontSize: "13px",
                color: "var(--mem-text-secondary)",
                lineHeight: 1.5,
              }}
            >
              {t("setup.connect.detectionFailed")}
            </p>
          )}

          {detectedClients.length > 0 && renderClientList(detectedClients, t("setup.connect.detected"), true)}
        </div>
      )}

      {supportedClients.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
          <SectionLabel>{t("setup.connect.supportedTools")}</SectionLabel>
          {renderClientList(supportedClients, t("setup.connect.installFirst"), false)}
        </div>
      )}

      <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
        <SectionLabel>{t("setup.connect.webTools")}</SectionLabel>
        <RemoteAccessPanel mode="compact" />
        <WebPlatformCards />
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
        <SectionLabel>{t("setup.connect.manualSetup")}</SectionLabel>
        <button
          onClick={() => setManualExpanded((prev) => !prev)}
          className="flex items-center gap-1.5 transition-colors duration-150 self-start"
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "13px",
            color: "var(--mem-text-secondary)",
          }}
        >
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            style={{
              transform: manualExpanded ? "rotate(90deg)" : "rotate(0deg)",
              transition: "transform 0.2s ease",
            }}
          >
            <polyline points="9 18 15 12 9 6" />
          </svg>
          {t("setup.connect.showConfigSnippet")}
        </button>

        {manualExpanded && (
          <div style={{ marginTop: "8px" }}>
            <p
              style={{
                fontFamily: "var(--mem-font-body)",
                fontSize: "12px",
                color: "var(--mem-text-tertiary)",
                marginBottom: "8px",
                lineHeight: "1.5",
              }}
            >
              {t("setup.connect.addConfigSnippet")}
            </p>
            <pre
              className="rounded-lg px-4 py-3"
              style={{
                fontFamily: "var(--mem-font-mono)",
                fontSize: "12px",
                color: "var(--mem-text)",
                backgroundColor: "var(--mem-surface)",
                border: "1px solid var(--mem-border)",
                overflow: "auto",
                lineHeight: "1.6",
              }}
            >
              {mcpJsonSnippet}
            </pre>
            <p
              style={{
                fontFamily: "var(--mem-font-body)",
                fontSize: "11px",
                color: "var(--mem-text-tertiary)",
                lineHeight: "1.5",
                marginTop: "6px",
              }}
            >
              {wenlanMcpEntry?.command === "npx"
                ? t("setup.connect.productionDefault")
                : t("setup.connect.developmentPath")}
            </p>
          </div>
        )}
      </div>

      <div
        className="flex items-center gap-3"
        style={{
          paddingTop: "16px",
          borderTop: "1px solid var(--mem-border)",
        }}
      >
        <button
          onClick={onNext}
          className="ml-auto transition-colors duration-150"
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "13px",
            color: "var(--mem-text-tertiary)",
            background: "none",
            border: "none",
            cursor: "pointer",
          }}
        >
          {t("setup.skip")}
        </button>
        <button
          onClick={handleContinue}
          disabled={isConnectingAll}
          className="px-5 py-2 rounded-lg text-sm font-medium transition-colors duration-150"
          style={{
            fontFamily: "var(--mem-font-body)",
            backgroundColor: "var(--mem-accent-indigo)",
            color: "white",
            opacity: isConnectingAll ? 0.7 : 1,
          }}
        >
          {isConnectingAll ? t("setup.connect.connecting") : t("setup.continue")}
        </button>
      </div>
    </div>
  );
}

// ── Verify Step ─────────────────────────────────────────────────────────

function VerifyStep({
  onNext,
  onBack,
  onConnected,
  wizardEnteredAt,
}: {
  onNext: () => void;
  onBack: () => void;
  onConnected: (agents: string[]) => void;
  wizardEnteredAt: number;
}) {
  const { t } = useTranslation();
  const [elapsed, setElapsed] = useState(0);
  const [advanced, setAdvanced] = useState(false);

  // Stable refs to avoid re-triggering the effect on parent re-renders
  const onNextRef = useRef(onNext);
  const onConnectedRef = useRef(onConnected);
  useEffect(() => { onNextRef.current = onNext; }, [onNext]);
  useEffect(() => { onConnectedRef.current = onConnected; }, [onConnected]);

  // Poll for agents every 3 seconds
  const { data: agents } = useQuery({
    queryKey: ["agents"],
    queryFn: listAgents,
    refetchInterval: 3000,
  });

  // Fresh-write proof: agents that wrote *since the wizard opened*. This is
  // the canonical success signal on a first-run install.
  const activeAgents = useMemo(
    () =>
      (agents ?? []).filter(
        (a) => a.last_seen_at != null && a.last_seen_at > wizardEnteredAt,
      ),
    [agents, wizardEnteredAt],
  );

  // Pre-existing connections: agents that have written at *any* point in the
  // past. On a re-run of the wizard (via Settings → reset onboarding), these
  // already-connected tools shouldn't make the user stare at a "waiting..."
  // screen — the success has already happened, just not in this wizard session.
  const preExistingAgents = useMemo(
    () =>
      (agents ?? []).filter(
        (a) => a.last_seen_at != null && a.name !== "unknown",
      ),
    [agents],
  );

  // Track elapsed time for progressive hints
  useEffect(() => {
    const interval = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(interval);
  }, []);

  // Auto-advance when an agent has written — either fresh since wizard start
  // (first-run happy path) or any time in the past (re-run short-circuit).
  // Fires once; refs break the effect/state dependency cycle.
  useEffect(() => {
    if (advanced) return;
    const winner = activeAgents.length > 0 ? activeAgents : preExistingAgents;
    if (winner.length > 0) {
      setAdvanced(true);
      onConnectedRef.current(winner.map((a) => a.name));
      onNextRef.current();
    }
  }, [activeAgents, preExistingAgents, advanced]);

  return (
    <div
      className="flex flex-col max-w-md mx-auto"
      style={{ gap: "24px", paddingTop: "24px" }}
    >
      {/* Back button */}
      <button
        onClick={onBack}
        className="flex items-center gap-1.5 self-start transition-colors duration-150"
        style={{
          fontFamily: "var(--mem-font-body)",
          fontSize: "13px",
          color: "var(--mem-text-secondary)",
        }}
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
        >
          <path d="M19 12H5M12 19l-7-7 7-7" />
        </svg>
        {t("setup.back")}
      </button>

      {/* Centered pulse + heading group */}
      <div
        className="flex flex-col items-center text-center"
        style={{ gap: "24px", paddingTop: "40px" }}
      >
      {/* Pulse dot */}
      <div
        style={{
          width: "48px",
          height: "48px",
          borderRadius: "50%",
          backgroundColor: "rgba(99, 102, 241, 0.15)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          animation: "pulse-ring 2s ease-in-out infinite",
        }}
      >
        <div
          style={{
            width: "16px",
            height: "16px",
            borderRadius: "50%",
            backgroundColor: "var(--mem-accent-indigo)",
          }}
        />
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
        <h1
          style={{
            fontFamily: "var(--mem-font-heading)",
            fontSize: "20px",
            fontWeight: 500,
            color: "var(--mem-text)",
          }}
        >
          {t("setup.verify.title")}
        </h1>
        <p
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "13px",
            color: "var(--mem-text-secondary)",
            lineHeight: "1.5",
          }}
        >
          {t("setup.verify.description")}
        </p>
      </div>

      {/* Progressive hints */}
      {elapsed >= 30 && elapsed < 60 && (
        <p
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "12px",
            color: "var(--mem-text-tertiary)",
            lineHeight: "1.5",
          }}
        >
          {t("setup.verify.restartHint")}
        </p>
      )}

      {elapsed >= 60 && (
        <div
          className="rounded-xl px-4 py-3"
          style={{
            backgroundColor: "var(--mem-surface)",
            border: "1px solid var(--mem-border)",
            textAlign: "left",
          }}
        >
          <p
            style={{
              fontFamily: "var(--mem-font-body)",
              fontSize: "13px",
              fontWeight: 500,
              color: "var(--mem-text)",
              marginBottom: "8px",
            }}
          >
            {t("setup.verify.troubleshootingTitle")}
          </p>
          <ul
            style={{
              fontFamily: "var(--mem-font-body)",
              fontSize: "12px",
              color: "var(--mem-text-secondary)",
              lineHeight: "1.8",
              paddingLeft: "16px",
              margin: 0,
            }}
          >
            <li>{t("setup.verify.troubleshootingServer")}</li>
            <li>{t("setup.verify.troubleshootingConfig")}</li>
            <li>{t("setup.verify.troubleshootingRestart")}</li>
          </ul>
        </div>
      )}

      {/* Skip link */}
      <button
        onClick={onNext}
        className="transition-colors duration-150"
        style={{
          fontFamily: "var(--mem-font-body)",
          fontSize: "13px",
          color: "var(--mem-text-tertiary)",
          background: "none",
          border: "none",
          cursor: "pointer",
        }}
      >
        {t("setup.skip")}
      </button>
      </div>

      {/* Pulse animation */}
      <style>{`
        @keyframes pulse-ring {
          0%, 100% { transform: scale(1); opacity: 1; }
          50% { transform: scale(1.1); opacity: 0.7; }
        }
      `}</style>
    </div>
  );
}

/** Small two-line stat cell used inside the Done step's import card. Paired
 *  with another to form a "Topics · Connections" row. Kept local to keep
 *  the Done variant self-contained. */
function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div
      className="flex flex-col flex-1 items-start"
      style={{ gap: "2px" }}
    >
      <span
        style={{
          fontFamily: "var(--mem-font-heading)",
          fontSize: "20px",
          fontWeight: 500,
          color: "var(--mem-text)",
          lineHeight: 1.1,
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {value}
      </span>
      <span
        style={{
          fontFamily: "var(--mem-font-mono)",
          fontSize: "10px",
          letterSpacing: "0.1em",
          textTransform: "uppercase",
          color: "var(--mem-text-tertiary)",
        }}
      >
        {label}
      </span>
    </div>
  );
}

// ── Done Step ───────────────────────────────────────────────────────────

function DoneStep({
  importResult,
  connectedAgents,
  onComplete,
}: {
  importResult: ImportResult | null;
  connectedAgents: string[];
  onComplete: () => void;
}) {
  const { t } = useTranslation();
  const hasImportData = importResult && importResult.imported > 0;
  const hasConnectedAgents = connectedAgents.length > 0;
  const isSkipPath = !hasImportData && !hasConnectedAgents;
  const breakdownEntries = hasImportData
    ? Object.entries(importResult.breakdown).filter(([, count]) => count > 0)
    : [];
  const kgTotal = hasImportData
    ? importResult.entities_created +
      importResult.observations_added +
      importResult.relations_created
    : 0;

  if (isSkipPath) {
    return (
      <div
        className="flex flex-col items-center text-center max-w-md mx-auto"
        style={{ gap: "28px", paddingTop: "32px" }}
      >
        <h1
          style={{
            fontFamily: "var(--mem-font-heading)",
            fontSize: "24px",
            fontWeight: 500,
            color: "var(--mem-text)",
          }}
        >
          {t("setup.done.readyTitle")}
        </h1>
        <p
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "14px",
            color: "var(--mem-text-secondary)",
            lineHeight: 1.6,
          }}
        >
          {t("setup.done.readyBody1")}
        </p>
        <p
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "14px",
            color: "var(--mem-text-secondary)",
            lineHeight: 1.6,
          }}
        >
          {t("setup.done.readyBody2")}
        </p>
        <button
          onClick={onComplete}
          className="px-6 py-2.5 rounded-lg text-sm font-medium"
          style={{
            fontFamily: "var(--mem-font-body)",
            backgroundColor: "var(--mem-accent-indigo)",
            color: "white",
            fontSize: "14px",
          }}
        >
          {t("setup.done.openWenlan")}
        </button>
      </div>
    );
  }

  return (
    <div
      className="flex flex-col items-center text-center max-w-md mx-auto"
      style={{ gap: "28px", paddingTop: "32px" }}
    >
      {/* Success icon */}
      <div
        style={{
          width: "56px",
          height: "56px",
          borderRadius: "50%",
          backgroundColor: "rgba(34, 197, 94, 0.15)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <svg
          width="28"
          height="28"
          viewBox="0 0 24 24"
          fill="none"
          stroke="rgb(34, 197, 94)"
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="20 6 9 17 4 12" />
        </svg>
      </div>

      {/* Beefed Done copy: headline + two prose lines */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "8px",
          textAlign: "center",
        }}
      >
        <p
          style={{
            fontFamily: "var(--mem-font-heading)",
            fontSize: "24px",
            fontWeight: 500,
            color: "var(--mem-text)",
            margin: 0,
          }}
        >
          {t("setup.done.allSetTitle")}
        </p>
        <p
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "14px",
            color: "var(--mem-text-secondary)",
            lineHeight: 1.6,
            margin: 0,
          }}
        >
          {t("setup.done.allSetBody1")}
        </p>
        <p
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "14px",
            color: "var(--mem-text-secondary)",
            lineHeight: 1.6,
            margin: 0,
          }}
        >
          {t("setup.done.allSetBody2")}
        </p>
      </div>

      {/* Import stats card — surfaces the essential value Wenlan extracted:
          memories in, topics (entities) pulled out, and a forward line about
          pages that will distill as the refinery runs. We rename
          "entities" → "topics" at the UI layer because that's the word end
          users think in; the data model keeps its canonical name. */}
      {hasImportData && (
        <div
          className="rounded-xl px-5 py-4 w-full text-left"
          style={{
            backgroundColor: "var(--mem-surface)",
            border: "1px solid var(--mem-border)",
            display: "flex",
            flexDirection: "column",
            gap: "12px",
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
            <p
              style={{
                fontFamily: "var(--mem-font-body)",
                fontSize: "14px",
                fontWeight: 500,
                color: "var(--mem-text)",
                margin: 0,
              }}
            >
              {t("setup.done.memoriesImported", { count: importResult.imported })}
            </p>

            {/* Type breakdown badges */}
            {breakdownEntries.length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {breakdownEntries.map(([type, count]) => (
                  <span
                    key={type}
                    className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs"
                    style={{
                      fontFamily: "var(--mem-font-body)",
                      backgroundColor: `color-mix(in srgb, ${MEMORY_TYPE_COLORS[type] || "var(--mem-text-tertiary)"} 15%, transparent)`,
                      color:
                        MEMORY_TYPE_COLORS[type] || "var(--mem-text-secondary)",
                    }}
                  >
                    {type}
                    <span style={{ opacity: 0.7 }}>{count}</span>
                  </span>
                ))}
              </div>
            )}
          </div>

          {/* Topics + connections — elevated from mono afterthought to a
              proper two-stat row. This is the knowledge graph surfacing
              into the onboarding: every topic is a potential anchor for a
              page. */}
          {kgTotal > 0 && (
            <div
              className="flex items-stretch"
              style={{
                borderTop: "1px solid var(--mem-border)",
                paddingTop: "12px",
                gap: "20px",
              }}
            >
              <Stat
                label={t("setup.done.topics")}
                value={importResult.entities_created}
              />
              <div style={{ width: "1px", backgroundColor: "var(--mem-border)" }} />
              <Stat
                label={t("setup.done.connections")}
                value={
                  importResult.observations_added +
                  importResult.relations_created
                }
              />
            </div>
          )}

          {/* Potential-pages expectation. Honest (no fabricated number):
              just the promise that distillation is next. */}
          <p
            style={{
              fontFamily: "var(--mem-font-body)",
              fontStyle: "italic",
              fontSize: "12.5px",
              color: "var(--mem-text-tertiary)",
              lineHeight: 1.5,
              margin: 0,
            }}
          >
            {t("setup.done.pagesWillDistill")}
          </p>
        </div>
      )}

      {/* Connected agents */}
      {connectedAgents.length > 0 && (
        <div
          className="flex flex-wrap items-center justify-center gap-2"
          style={{ marginTop: "-8px" }}
        >
          <span
            style={{
              fontFamily: "var(--mem-font-body)",
              fontSize: "13px",
              color: "var(--mem-text-secondary)",
            }}
          >
            {t("setup.done.connected")}
          </span>
          {connectedAgents.map((name) => (
            <span
              key={name}
              className="px-2 py-0.5 rounded-full text-xs"
              style={{
                fontFamily: "var(--mem-font-body)",
                backgroundColor: "rgba(99, 102, 241, 0.12)",
                color: "var(--mem-accent-indigo)",
              }}
            >
              {name}
            </span>
          ))}
        </div>
      )}

      {/* Get started button */}
      <button
        onClick={onComplete}
        className="px-6 py-2.5 rounded-lg text-sm font-medium transition-colors duration-150"
        style={{
          fontFamily: "var(--mem-font-body)",
          backgroundColor: "var(--mem-accent-indigo)",
          color: "white",
          fontSize: "14px",
        }}
      >
        {t("setup.getStarted")}
      </button>
    </div>
  );
}

// ── SetupWizard ─────────────────────────────────────────────────────────

export function SetupWizard({ onComplete, initialStep }: SetupWizardProps) {
  const startStep = initialStep ?? "welcome";
  const [step, setStep] = useState<WizardStep>(startStep);
  const [importResult, setImportResult] = useState<ImportResult | null>(null);
  const [connectedAgents, setConnectedAgents] = useState<string[]>([]);
  const [, setImportPhase] = useState<string>("input");
  const wizardEnteredAtRef = useRef<number>(Math.floor(Date.now() / 1000));

  const handleConnectedAgents = (agents: string[]) => {
    setConnectedAgents((prev) => {
      const merged = new Set([...prev, ...agents]);
      return Array.from(merged);
    });
  };

  return (
    <div
      className="flex flex-col"
      style={{
        height: "100vh",
        backgroundColor: "var(--mem-bg)",
        overflow: "auto",
      }}
    >
      {/* Drag region */}
      <div
        data-tauri-drag-region
        style={{
          height: "32px",
          flexShrink: 0,
        }}
      />

      {/* Step indicator — hide when entering at a specific step */}
      {!initialStep && <StepIndicator currentStep={step} />}

      {/* Step content */}
      <div
        className="flex-1"
        style={{
          maxWidth: "640px",
          width: "100%",
          margin: "0 auto",
          padding: "0 24px 48px",
        }}
      >
        {step === "welcome" && (
          <WelcomeStep onNext={() => setStep("intelligence-choice")} />
        )}

        {step === "intelligence-choice" && (
          <IntelligenceChoiceStep
            onBack={() => setStep("welcome")}
            onNext={() => setStep("import")}
          />
        )}

        {step === "import" && (
          <ImportStep
            onBack={() => setStep("intelligence-choice")}
            importHint={(
              <Trans
                i18nKey="setup.import.laterHint"
                components={{ strong: <strong /> }}
              />
            )}
            onPhaseChange={setImportPhase}
            onSkip={() => setStep("connect")}
            onComplete={(_source, result) => {
              setImportResult(result);
              setStep("connect");
            }}
          />
        )}

        {step === "connect" && (
          <ConnectStep
            onNext={() => setStep("verify")}
            onBack={startStep === "connect" ? onComplete : () => setStep("import")}
            onConnected={handleConnectedAgents}
          />
        )}

        {step === "verify" && (
          <VerifyStep
            onNext={() => setStep("done")}
            onBack={() => setStep("connect")}
            onConnected={handleConnectedAgents}
            wizardEnteredAt={wizardEnteredAtRef.current}
          />
        )}

        {step === "done" && (
          <DoneStep
            importResult={importResult}
            connectedAgents={connectedAgents}
            onComplete={onComplete}
          />
        )}
      </div>
    </div>
  );
}

export default SetupWizard;
