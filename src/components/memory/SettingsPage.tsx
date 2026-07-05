// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import {
  getProfile,
  updateProfile,
  setAvatar,
  removeAvatar,
  getClipboardEnabled,
  setClipboardEnabled,
  getScreenCaptureEnabled,
  setScreenCaptureEnabled,
  checkScreenPermission,
  requestScreenPermission,
  getCaptureStats,
  listAgents,
  updateAgent,
  deleteAgent,
  detectMcpClients,
  setSetupCompleted,
  isRunAtLoginEnabled,
  setRunAtLogin,
} from "../../lib/tauri";
import { type Theme, useTheme } from "../../lib/theme";
import {
  readStoredLocalePreference,
  setLocalePreference,
  type StoredLocale,
} from "../../i18n";
import { describeTrustLevel, resolveAgentDisplayName, TRUST_LEVELS } from "../../lib/agents";
import SourcesSection from "./sources/SourcesSection";
import { ImportFlow } from "../ChatImport/ImportFlow";
import { RemoteAccessPanel } from "./RemoteAccessPanel";
import { ApiKeyCard, OnDeviceModelCard, useApiKeyStatus } from "../intelligence/IntelligenceSetup";
import DiagnosticsSection from "./settings/DiagnosticsSection";
import ProfileAvatar from "./ProfileAvatar";

type ThemeLabelKey =
  | "settings.theme.auto"
  | "settings.theme.light"
  | "settings.theme.dark";

const THEME_OPTIONS: { value: Theme; labelKey: ThemeLabelKey; icon: React.ReactNode }[] = [
  {
    value: "system",
    labelKey: "settings.theme.auto",
    icon: (
      <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
      </svg>
    ),
  },
  {
    value: "light",
    labelKey: "settings.theme.light",
    icon: (
      <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
      </svg>
    ),
  },
  {
    value: "dark",
    labelKey: "settings.theme.dark",
    icon: (
      <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
      </svg>
    ),
  },
];

// ── Helpers ──────────────────────────────────────────────────────────────

function Toggle({ enabled, onToggle }: { enabled: boolean; onToggle: () => void }) {
  return (
    <button
      onClick={onToggle}
      className={`relative w-11 h-[26px] rounded-full transition-colors shrink-0 ${
        enabled ? "bg-[var(--mem-accent-indigo)]" : "bg-[var(--mem-hover-strong)]"
      }`}
    >
      <span
        className={`absolute top-[3px] w-5 h-5 rounded-full bg-white shadow-sm transition-transform ${
          enabled ? "left-[22px]" : "left-[3px]"
        }`}
      />
    </button>
  );
}

function SettingRow({
  title,
  description,
  enabled,
  onToggle,
  statusLine,
  warning,
  error,
}: {
  title: string;
  description: string;
  enabled: boolean;
  onToggle: () => void;
  statusLine?: React.ReactNode;
  warning?: string | null;
  error?: string | null;
}) {
  return (
    <div className="px-5 py-4">
      <div className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <div style={{ fontFamily: "var(--mem-font-body)", fontSize: "14px", fontWeight: 500, color: "var(--mem-text)" }}>
            {title}
          </div>
          <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", marginTop: "2px", lineHeight: "1.5" }}>
            {description}
          </p>
        </div>
        <div className="mt-0.5">
          <Toggle enabled={enabled} onToggle={onToggle} />
        </div>
      </div>
      {statusLine && <div className="mt-2">{statusLine}</div>}
      {error && (
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "#ef4444", marginTop: "8px", lineHeight: "1.5", wordBreak: "break-all" }}>
          {error}
        </p>
      )}
      {warning && (
        <div className="flex items-start gap-2 mt-2">
          <svg className="w-3.5 h-3.5 text-amber-400 shrink-0 mt-px" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.072 16.5c-.77.833.192 2.5 1.732 2.5z" />
          </svg>
          <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "#fbbf24", lineHeight: "1.5" }}>{warning}</p>
        </div>
      )}
    </div>
  );
}

function SectionHeader({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <div className="flex items-center gap-2 mb-3 px-1">
      <span style={{ color: "var(--mem-text-tertiary)" }}>{icon}</span>
      <h3 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "12px", fontWeight: 600, letterSpacing: "0.05em", color: "var(--mem-text-tertiary)", textTransform: "uppercase" as const }}>
        {label}
      </h3>
    </div>
  );
}

function formatProfileMonth(ts: number): string {
  return new Intl.DateTimeFormat(undefined, {
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  }).format(ts * 1000);
}

interface ProfileUpdateFields {
  name?: string;
  displayName?: string;
  bio?: string;
}

function ProfileSettingsBlock() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: profile } = useQuery({
    queryKey: ["profile"],
    queryFn: getProfile,
  });
  const [nameDraft, setNameDraft] = useState("");

  const displayName = profile?.display_name || profile?.name || "";

  useEffect(() => {
    setNameDraft(displayName);
  }, [displayName]);

  const profileMutation = useMutation({
    mutationFn: (fields: ProfileUpdateFields) => {
      if (!profile) return Promise.resolve();
      return updateProfile(profile.id, fields.name, fields.displayName, undefined, fields.bio);
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["profile"] }),
  });

  const avatarMutation = useMutation({
    mutationFn: (path: string) => setAvatar(path),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["profile"] }),
  });

  const removeAvatarMutation = useMutation({
    mutationFn: removeAvatar,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["profile"] }),
  });

  const saveName = () => {
    if (!profile) return;
    const next = nameDraft.trim();
    const current = displayName.trim();
    if (!next || next === current) {
      setNameDraft(displayName);
      return;
    }
    profileMutation.mutate({ name: next, displayName: next });
  };

  const handlePickAvatar = async () => {
    const selected = await open({
      title: t("settings.profile.choosePhoto"),
      filters: [{ name: t("settings.profile.images"), extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
    });
    if (typeof selected === "string") {
      avatarMutation.mutate(selected);
    }
  };

  if (!profile) return null;

  return (
    <section className="mem-fade-up" style={{ animationDelay: "0ms" }}>
      <SectionHeader
        label={t("settings.profile.label")}
        icon={
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M20 21a8 8 0 10-16 0" />
            <circle cx="12" cy="7" r="4" strokeWidth={1.5} />
          </svg>
        }
      />
      <div className="bg-[var(--mem-surface)] rounded-xl border border-[var(--mem-border)] px-5 py-4">
        <div className="flex items-start gap-4">
          <div className="flex shrink-0 flex-col items-center gap-2">
            <ProfileAvatar
              avatarPath={profile.avatar_path}
              displayName={displayName}
              size={56}
              fontSize={20}
            />
            <button
              type="button"
              onClick={handlePickAvatar}
              className="rounded-md px-2 py-1 transition-colors duration-150 hover:bg-[var(--mem-hover)]"
              style={{
                fontFamily: "var(--mem-font-body)",
                fontSize: "11px",
                color: "var(--mem-text-secondary)",
              }}
            >
              {t("settings.profile.changePhoto")}
            </button>
            {profile.avatar_path && (
              <button
                type="button"
                onClick={() => removeAvatarMutation.mutate()}
                className="rounded-md px-2 py-1 transition-colors duration-150 hover:bg-[var(--mem-hover)]"
                style={{
                  fontFamily: "var(--mem-font-body)",
                  fontSize: "11px",
                  color: "var(--mem-text-tertiary)",
                }}
              >
                {t("settings.profile.removePhoto")}
              </button>
            )}
          </div>

          <div className="min-w-0 flex-1 space-y-3">
            <label className="block">
              <span
                style={{
                  display: "block",
                  fontFamily: "var(--mem-font-mono)",
                  fontSize: "10px",
                  fontWeight: 600,
                  letterSpacing: "0.05em",
                  textTransform: "uppercase",
                  color: "var(--mem-text-tertiary)",
                  marginBottom: 4,
                }}
              >
                {t("settings.profile.displayName")}
              </span>
              <input
                value={nameDraft}
                onChange={(event) => setNameDraft(event.target.value)}
                onBlur={saveName}
                onKeyDown={(event) => {
                  if (event.key === "Enter") event.currentTarget.blur();
                  if (event.key === "Escape") {
                    setNameDraft(displayName);
                    event.currentTarget.blur();
                  }
                }}
                className="w-full rounded-lg border px-3 py-2 outline-none transition-colors duration-150 focus:border-[var(--mem-accent-indigo)]"
                style={{
                  borderColor: "var(--mem-border)",
                  backgroundColor: "var(--mem-bg)",
                  color: "var(--mem-text)",
                  fontFamily: "var(--mem-font-body)",
                  fontSize: "14px",
                }}
              />
            </label>

            <p
              style={{
                fontFamily: "var(--mem-font-mono)",
                fontSize: "11px",
                color: "var(--mem-text-tertiary)",
              }}
            >
              {t("settings.profile.joined", { date: formatProfileMonth(profile.created_at) })}
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

// ── Main component ─────────────────────────────────────────────────────

import type { SettingsSection } from "./settings/SettingsSidebar";
import { SETTINGS_GROUPS } from "./settings/SettingsSidebar";

interface SettingsPageProps {
  /** Which group to display. Driven by the Settings sidebar in Main.tsx. */
  section?: SettingsSection;
  onBack: () => void;
  onSetupAgent?: () => void;
  onImport?: () => void;
}

export default function SettingsPage({
  section = "general",
  onBack,
  onSetupAgent,
  onImport,
}: SettingsPageProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [theme, setThemeValue] = useTheme();
  const [languagePreference, setLanguagePreference] = useState<StoredLocale>(
    () => readStoredLocalePreference(),
  );

  // ── Source toggles ─────────────────────────────────────────────────
  const { data: clipboardEnabled = true } = useQuery({
    queryKey: ["clipboardEnabled"],
    queryFn: getClipboardEnabled,
  });

  const clipboardToggleMutation = useMutation({
    mutationFn: setClipboardEnabled,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["clipboardEnabled"] }),
  });

  // ── Screen capture ─────────────────────────────────────────────────
  const { data: screenCaptureEnabled = false } = useQuery({
    queryKey: ["screenCaptureEnabled"],
    queryFn: getScreenCaptureEnabled,
  });

  const screenCaptureMutation = useMutation({
    mutationFn: setScreenCaptureEnabled,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["screenCaptureEnabled"] }),
  });

  const { data: screenPermission = false, refetch: refetchPermission } = useQuery({
    queryKey: ["screenPermission"],
    queryFn: checkScreenPermission,
    refetchInterval: 5000, // re-check after user grants in System Settings
  });

  const { data: captureStats } = useQuery({
    queryKey: ["captureStats"],
    queryFn: getCaptureStats,
    refetchInterval: 5000,
  });

  // ── Run at login ───────────────────────────────────────────────────
  const runAtLoginQuery = useQuery({
    queryKey: ["runAtLogin"],
    queryFn: isRunAtLoginEnabled,
  });
  const runAtLoginMutation = useMutation({
    mutationFn: setRunAtLogin,
    onSuccess: () => runAtLoginQuery.refetch(),
  });

  // Filters removed — legacy ambient capture, hidden from UI

  // ── Connected Agents ───────────────────────────────────────────────
  const { data: agents = [] } = useQuery({
    queryKey: ["agents"],
    queryFn: listAgents,
  });

  // Detect configured MCP clients to show pending connections
  const { data: mcpClients = [] } = useQuery({
    queryKey: ["mcp-clients"],
    queryFn: detectMcpClients,
  });

  // Show configured clients that have not written a first memory yet.
  const registeredClientTypes = new Set(agents.map((agent) => agent.agent_type));
  const pendingClients = mcpClients.filter(
    (client) => client.already_configured && !registeredClientTypes.has(client.client_type),
  );

  const updateAgentMut = useMutation({
    mutationFn: ({ name, updates }: { name: string; updates: { enabled?: boolean; trustLevel?: string } }) =>
      updateAgent(name, updates),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["agents"] }),
  });

  const deleteAgentMut = useMutation({
    mutationFn: deleteAgent,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["agents"] }),
  });

  const [deletingAgent, setDeletingAgent] = useState<string | null>(null);

  // (old `sections` array deleted — replaced by the SettingsSidebar driving
  // a single active group via the `section` prop.)

  const activeGroup = SETTINGS_GROUPS.find((g) => g.id === section);

  return (
    <div className="flex flex-col gap-6 max-w-2xl mx-auto py-4">
      {/* Back + Heading. The heading now names the active group; the sidebar
          handles navigating between groups. */}
      <div>
        <button onClick={onBack} className="p-1.5 -ml-1.5 rounded-md transition-colors duration-150 hover:bg-[var(--mem-hover)]" style={{ color: "var(--mem-text-tertiary)", background: "none", border: "none", cursor: "pointer", lineHeight: 0, marginBottom: "12px" }}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M19 12H5M12 19l-7-7 7-7" /></svg>
        </button>
        <h1 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "20px", fontWeight: 500, color: "var(--mem-text)" }}>
          {activeGroup ? t(activeGroup.labelKey) : t("settings.title")}
        </h1>
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", color: "var(--mem-text-secondary)", marginTop: "4px" }}>
          {activeGroup ? t(activeGroup.hintKey) : t("settings.manageHint")}
        </p>
      </div>

      {/* ── Appearance ───────────────────────────────────────────── */}
      {/* ── General ─────────────────────────────────────────────── */}
      {section === "general" && (
      <>
      <ProfileSettingsBlock />
      <section className="mem-fade-up" style={{ animationDelay: "0ms" }}>
        <SectionHeader
          label={t("settings.general.appSection")}
          icon={<svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><circle cx="12" cy="12" r="3" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 010 2.83 2 2 0 01-2.83 0l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 01-2.83-2.83l.06-.06A1.65 1.65 0 004.68 15a1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 012.83-2.83l.06.06A1.65 1.65 0 009 4.68a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 012.83 2.83l-.06.06A1.65 1.65 0 0019.4 9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z" /></svg>}
        />
        <div className="bg-[var(--mem-surface)] rounded-xl overflow-hidden border border-[var(--mem-border)]">
          <SettingRow
            title={t("settings.general.runAtLoginTitle")}
            description={t("settings.general.runAtLoginDescription")}
            enabled={runAtLoginQuery.data ?? false}
            onToggle={() => runAtLoginMutation.mutate(!(runAtLoginQuery.data ?? false))}
          />
        </div>
        {/* Theme — folded into General; previously its own "Appearance" sidebar entry. */}
        <div className="bg-[var(--mem-surface)] rounded-xl overflow-hidden border border-[var(--mem-border)] mt-4">
          <div className="px-5 py-4">
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0">
                <div style={{ fontFamily: "var(--mem-font-body)", fontSize: "14px", fontWeight: 500, color: "var(--mem-text)" }}>{t("settings.theme.label")}</div>
                <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", marginTop: "2px", lineHeight: "1.5" }}>
                  {t("settings.theme.description")}
                </p>
              </div>
              <div className="relative flex bg-[var(--mem-hover)] rounded-lg p-0.5 shrink-0">
                <div
                  className="absolute top-0.5 bottom-0.5 rounded-md shadow-sm transition-transform duration-200 ease-out"
                  style={{
                    backgroundColor: "var(--mem-accent-indigo)",
                    width: `calc(${100 / THEME_OPTIONS.length}% - 2px)`,
                    transform: `translateX(calc(${THEME_OPTIONS.findIndex((o) => o.value === theme)} * (100% + ${4 / (THEME_OPTIONS.length - 1)}px)))`,
                    left: 1,
                  }}
                />
                {THEME_OPTIONS.map((opt) => (
                  <button
                    key={opt.value}
                    onClick={() => setThemeValue(opt.value)}
                    className={`relative z-10 flex items-center gap-1.5 px-3 py-1.5 rounded-md transition-colors duration-200 ${
                      theme === opt.value
                        ? "text-white"
                        : "text-[var(--mem-text-secondary)] hover:text-[var(--mem-text)]"
                    }`}
                    style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", fontWeight: 500 }}
                  >
                    {opt.icon}
                    {t(opt.labelKey)}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>
        <div className="bg-[var(--mem-surface)] rounded-xl overflow-hidden border border-[var(--mem-border)] mt-4">
          <div className="px-5 py-4">
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0">
                <label
                  htmlFor="settings-language"
                  style={{ fontFamily: "var(--mem-font-body)", fontSize: "14px", fontWeight: 500, color: "var(--mem-text)" }}
                >
                  {t("settings.language.label")}
                </label>
                <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", marginTop: "2px", lineHeight: "1.5" }}>
                  {t("settings.language.description")}
                </p>
              </div>
              <select
                id="settings-language"
                value={languagePreference}
                onChange={(event) => {
                  const nextPreference = event.currentTarget.value as StoredLocale;
                  setLanguagePreference(nextPreference);
                  void setLocalePreference(nextPreference);
                }}
                className="rounded-lg px-3 py-1.5 shrink-0"
                style={{
                  backgroundColor: "var(--mem-hover)",
                  border: "1px solid var(--mem-border)",
                  color: "var(--mem-text)",
                  fontFamily: "var(--mem-font-body)",
                  fontSize: "12px",
                }}
              >
                <option value="system">{t("settings.language.system")}</option>
                <option value="en">{t("settings.language.english")}</option>
                <option value="zh-Hans">{t("settings.language.simplifiedChinese")}</option>
                <option value="zh-Hant">{t("settings.language.traditionalChinese")}</option>
              </select>
            </div>
          </div>
        </div>
        {/* Re-run setup wizard. Confirmation prevents accidental restart;
            data is preserved regardless. */}
        <div className="px-2 pt-4">
          <button
            onClick={async () => {
              const ok = window.confirm(
                t("settings.general.rerunSetupConfirm")
              );
              if (!ok) return;
              await setSetupCompleted(false);
              queryClient.invalidateQueries({ queryKey: ["shouldShowWizard"] });
            }}
            className="transition-colors hover:underline"
            style={{
              fontFamily: "var(--mem-font-body)",
              fontSize: "13px",
              color: "var(--mem-text-secondary)",
            }}
          >
            {t("settings.general.rerunSetup")}
          </button>
        </div>
      </section>
      </>
      )}

      {/* ── Capture ─────────────────────────────────────────────── */}
      {section === "capture" && (
      <section className="mem-fade-up" style={{ animationDelay: "0ms" }}>
        <SectionHeader
          label={t("settings.capture.label")}
          icon={<svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z" /><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 13a3 3 0 11-6 0 3 3 0 016 0z" /></svg>}
        />
        <div className="bg-[var(--mem-surface)] rounded-xl overflow-hidden border border-[var(--mem-border)]">
          {/* Clipboard Capture */}
          <SettingRow
            title={t("settings.capture.clipboardTitle")}
            description={t("settings.capture.clipboardDescription")}
            enabled={clipboardEnabled}
            onToggle={() => clipboardToggleMutation.mutate(!clipboardEnabled)}
            statusLine={
              captureStats && captureStats.clipboard > 0 ? (
                <span style={{ fontFamily: "var(--mem-font-mono)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}>
                  {t("settings.capture.itemsCaptured", { count: captureStats.clipboard })}
                </span>
              ) : null
            }
          />

          <div className="mx-5 border-t border-[var(--mem-border)]" style={{ opacity: 0.4 }} />

          {/* Screen Capture */}
          <div className="px-5 py-4">
            <div className="flex items-start justify-between gap-4">
              <div className="min-w-0">
                <div style={{ fontFamily: "var(--mem-font-body)", fontSize: "14px", fontWeight: 500, color: "var(--mem-text)" }}>
                  {t("settings.capture.screenTitle")}
                </div>
                <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", marginTop: "2px", lineHeight: "1.5" }}>
                  {t("settings.capture.screenDescription")}
                </p>
              </div>
              <div className="mt-0.5">
                <Toggle
                  enabled={screenCaptureEnabled}
                  onToggle={() => {
                    const next = !screenCaptureEnabled;
                    if (next && !screenPermission) {
                      // Request permission first, then enable
                      (async () => {
                        await requestScreenPermission();
                        setTimeout(() => refetchPermission(), 1000);
                      })();
                    }
                    screenCaptureMutation.mutate(next);
                  }}
                />
              </div>
            </div>
            {/* Permission status inline */}
            <div className="flex items-center gap-2 mt-2.5">
              <span
                className="w-2 h-2 rounded-full shrink-0"
                style={{ background: screenPermission ? "var(--mem-accent-sage)" : "#ef4444" }}
              />
              <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}>
                {t("settings.capture.screenRecording")} {screenPermission ? t("settings.capture.granted") : t("settings.capture.notGranted")}
              </span>
              {!screenPermission && (
                <button
                  onClick={async () => {
                    await requestScreenPermission();
                    setTimeout(() => refetchPermission(), 1000);
                  }}
                  className="px-2 py-0.5 rounded text-[10px] font-medium transition-colors"
                  style={{
                    fontFamily: "var(--mem-font-body)",
                    background: "var(--mem-hover-strong)",
                    color: "var(--mem-text)",
                  }}
                >
                  {t("settings.capture.grantAccess")}
                </button>
              )}
            </div>
          </div>

        </div>
      </section>
      )}

      {/* ── Sources (group: sources) ────────────────────────────── */}
      {section === "sources" && (<>
      {/* ── Import Memories ────────────────────────────────────── */}
      <section className="mem-fade-up" style={{ animationDelay: "0ms" }}>
        <SectionHeader
          label={t("settings.sources.importMemoriesTitle")}
          icon={<svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" /></svg>}
        />
        <div className="bg-[var(--mem-surface)] rounded-xl overflow-hidden border border-[var(--mem-border)]">
          <div className="px-5 py-4">
            <div className="flex items-center justify-between">
              <div className="min-w-0">
                <div style={{ fontFamily: "var(--mem-font-body)", fontSize: "14px", fontWeight: 500, color: "var(--mem-text)" }}>{t("settings.sources.importMemoriesTitle")}</div>
                <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", marginTop: "2px", lineHeight: "1.5" }}>
                  {t("settings.sources.importMemoriesDescription")}
                </p>
              </div>
              {onImport && (
                <button
                  onClick={onImport}
                  className="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors duration-150 shrink-0"
                  style={{ fontFamily: "var(--mem-font-body)", backgroundColor: "var(--mem-accent-indigo)", color: "white" }}
                >
                  {t("settings.sources.import")}
                </button>
              )}
            </div>
          </div>
        </div>
      </section>

      {/* ── Import Chat History ──────────────────────────────── */}
      <section className="mem-fade-up" style={{ animationDelay: "60ms" }}>
        <SectionHeader
          label={t("settings.sources.importChatHistoryTitle")}
          icon={<svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 4H6a2 2 0 00-2 2v12a2 2 0 002 2h12a2 2 0 002-2V6a2 2 0 00-2-2h-2m-4 0v4m0-4L8 8m4-4l4 4" /></svg>}
        />
        <div className="bg-[var(--mem-surface)] rounded-xl overflow-hidden border border-[var(--mem-border)]">
          <div className="px-5 py-4">
            <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", marginBottom: "12px", lineHeight: "1.5" }}>
              {t("settings.sources.importChatHistoryDescription")}
            </p>
            <ImportFlow />
          </div>
        </div>
      </section>

      {/* ── Sources ────────────────────────────────────────────── */}
      <section className="mem-fade-up" style={{ animationDelay: "30ms" }}>
        <SectionHeader
          label={t("settings.groups.sources.label")}
          icon={<svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4" /></svg>}
        />
        <SourcesSection />
      </section>
      </>)}

      {/* ── Agents (group: agents) ─────────────────────────────── */}
      {section === "agents" && (<>
      {/* ── Connected Agents ─────────────────────────────────────── */}
      <section className="mem-fade-up" style={{ animationDelay: "0ms" }}>
        <SectionHeader
          icon={
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
            </svg>
          }
          label={t("settings.agents.connectedAgents")}
        />
        {/* Trust level explainer — makes it clear what the badges mean and how
            Wenlan gates context for each tier. */}
        <div
          className="rounded-xl mb-3 px-4 py-3"
          style={{
            backgroundColor: "var(--mem-hover)",
            border: "1px solid var(--mem-border)",
          }}
        >
          <p
            style={{
              fontFamily: "var(--mem-font-mono)",
              fontSize: "10px",
              fontWeight: 600,
              letterSpacing: "0.05em",
              textTransform: "uppercase",
              color: "var(--mem-text-tertiary)",
              marginBottom: 8,
            }}
          >
            {t("settings.agents.trustLevels")}
          </p>
          <div className="flex flex-col gap-1.5">
            {(Object.keys(TRUST_LEVELS) as Array<keyof typeof TRUST_LEVELS>).map((level) => {
              const d = TRUST_LEVELS[level];
              return (
                <div key={level} className="flex items-start gap-2">
                  <span
                    className="shrink-0 px-1.5 py-0.5 rounded"
                    style={{
                      fontFamily: "var(--mem-font-mono)",
                      fontSize: "10px",
                      fontWeight: 500,
                      color: d.accent,
                      border: `1px solid ${d.accent}`,
                      backgroundColor: "transparent",
                      minWidth: 52,
                      textAlign: "center",
                    }}
                  >
                    {d.label}
                  </span>
                  <span
                    style={{
                      fontFamily: "var(--mem-font-body)",
                      fontSize: "12px",
                      color: "var(--mem-text-secondary)",
                      lineHeight: 1.5,
                    }}
                  >
                    {d.summary}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
        <div className="rounded-xl bg-[var(--mem-surface)] border border-[var(--mem-border)]">
          {agents.length === 0 && pendingClients.length === 0 ? (
            <div className="px-5 py-6 text-center space-y-3">
              <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", color: "var(--mem-text-tertiary)" }}>
                {t("settings.agents.noAgents")}
              </p>
              <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-tertiary)", opacity: 0.7, lineHeight: "1.5" }}>
                {t("settings.agents.noAgentsDescription")}
              </p>
              {onSetupAgent && (
                <button
                  onClick={onSetupAgent}
                  className="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors duration-150"
                  style={{
                    fontFamily: "var(--mem-font-body)",
                    backgroundColor: "var(--mem-accent-indigo)",
                    color: "white",
                  }}
                >
                  {t("settings.agents.setupTool")}
                </button>
              )}
            </div>
          ) : (
            <div className="divide-y divide-[var(--mem-border)]">
              {pendingClients.map((client) => (
                <div
                  key={client.client_type}
                  className="px-5 py-3"
                  style={{
                    opacity: 0.7,
                  }}
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "14px", fontWeight: 500, color: "var(--mem-text)" }}>
                          {client.name}
                        </span>
                        <span
                          className="px-1.5 py-0.5 rounded"
                          style={{ fontFamily: "var(--mem-font-mono)", fontSize: "10px", backgroundColor: "rgba(251, 191, 36, 0.1)", color: "var(--mem-accent-amber)" }}
                        >
                          {t("settings.agents.configured")}
                        </span>
                      </div>
                      <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)", marginTop: "2px" }}>
                        {t("settings.agents.restartToActivate", { name: client.name })}
                      </p>
                    </div>
                  </div>
                </div>
              ))}
              {agents.map((agent) => (
                <div
                  key={agent.id}
                  className="px-5 py-3 transition-opacity"
                  style={{
                    /* no left border strip */
                    opacity: agent.enabled ? 1 : 0.5,
                  }}
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2 flex-wrap">
                        {/* Display name (prominent, what the user cares about) */}
                        <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "14px", fontWeight: 500, color: "var(--mem-text)" }}>
                          {resolveAgentDisplayName(agent.name, agents)}
                        </span>
                        <span
                          className="px-1.5 py-0.5 rounded"
                          style={{ fontFamily: "var(--mem-font-mono)", fontSize: "10px", backgroundColor: "var(--mem-hover)", color: "var(--mem-text-tertiary)" }}
                        >
                          {agent.agent_type}
                        </span>
                        {/* Trust badge lives in the right-hand action cluster
                            below — it's a styled `<select>` that doubles as
                            both the display and the editor. */}
                      </div>
                      <div className="flex items-center gap-3 mt-1 flex-wrap">
                        {/* Canonical technical ID (secondary, only if it differs from display) */}
                        {resolveAgentDisplayName(agent.name, agents) !== agent.name && (
                          <span
                            title={t("settings.agents.canonicalIdTitle")}
                            style={{
                              fontFamily: "var(--mem-font-mono)",
                              fontSize: "10px",
                              color: "var(--mem-text-tertiary)",
                              opacity: 0.75,
                            }}
                          >
                            {agent.name}
                          </span>
                        )}
                        <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}>
                          {t("settings.agents.memories", { count: agent.memory_count })}
                        </span>
                        {agent.last_seen_at && (
                          <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}>
                            {t("settings.agents.lastSeen", {
                              date: new Date(agent.last_seen_at * 1000).toLocaleDateString(),
                            })}
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="flex items-center gap-2 shrink-0">
                      {/* Trust selector — the <select> is wrapped so a chevron
                          can sit as a sibling (selects don't accept reliable
                          pseudo-elements across browsers). The chevron carries
                          the affordance; the pill border matches the legend
                          above. Hovering shows the level's summary as a
                          tooltip AND tints the background subtly so the
                          control clearly reads as interactive. */}
                      {(() => {
                        const d = describeTrustLevel(agent.trust_level);
                        return (
                          <div
                            className="relative shrink-0 inline-flex"
                            title={d.summary}
                          >
                            <select
                              value={agent.trust_level}
                              onChange={(e) =>
                                updateAgentMut.mutate({
                                  name: agent.name,
                                  updates: { trustLevel: e.target.value },
                                })
                              }
                              className="rounded focus:outline-none cursor-pointer transition-colors duration-150"
                              style={{
                                fontFamily: "var(--mem-font-mono)",
                                fontSize: "10px",
                                fontWeight: 500,
                                color: d.accent,
                                border: `1px solid ${d.accent}`,
                                backgroundColor: "transparent",
                                minWidth: 56,
                                // Asymmetric padding — paddingRight reserves
                                // room for the chevron so the closed-state
                                // text doesn't collide with it. `textAlignLast`
                                // centers the visible value within the
                                // content area; the slight left bias compensates
                                // for the chevron's visual weight on the right.
                                textAlign: "center",
                                textAlignLast: "center",
                                paddingTop: 4,
                                paddingBottom: 4,
                                paddingLeft: 8,
                                paddingRight: 17,
                                appearance: "none",
                                WebkitAppearance: "none",
                                MozAppearance: "none",
                                backgroundImage: "none",
                                lineHeight: 1.2,
                              }}
                              onMouseEnter={(e) => {
                                // Subtle tint so the badge reads as "press me".
                                // `currentColor` isn't easy to reference in
                                // inline styles, so we rebuild the rgba from
                                // the accent var at hover time via the browser.
                                (e.currentTarget as HTMLElement).style.backgroundColor =
                                  "var(--mem-hover)";
                              }}
                              onMouseLeave={(e) => {
                                (e.currentTarget as HTMLElement).style.backgroundColor =
                                  "transparent";
                              }}
                            >
                              <option value="full">{t("settings.agents.trust.full")}</option>
                              <option value="review">{t("settings.agents.trust.review")}</option>
                              <option value="unknown">{t("settings.agents.trust.unknown")}</option>
                            </select>
                            {/* Chevron — absolutely positioned, pointer-events:none
                                so clicks pass through to the select. Color
                                matches the trust accent so the whole control
                                reads as one unit. */}
                            <svg
                              width="8"
                              height="8"
                              viewBox="0 0 8 8"
                              fill="none"
                              stroke={d.accent}
                              strokeWidth="1.5"
                              strokeLinecap="round"
                              strokeLinejoin="round"
                              aria-hidden
                              style={{
                                position: "absolute",
                                right: 6,
                                top: "50%",
                                transform: "translateY(-50%)",
                                pointerEvents: "none",
                              }}
                            >
                              <polyline points="1.5 3 4 5.5 6.5 3" />
                            </svg>
                          </div>
                        );
                      })()}
                      <Toggle
                        enabled={agent.enabled}
                        onToggle={() =>
                          updateAgentMut.mutate({
                            name: agent.name,
                            updates: { enabled: !agent.enabled },
                          })
                        }
                      />
                      {deletingAgent === agent.name ? (
                        <div className="flex items-center gap-1">
                          <button
                            onClick={() => {
                              deleteAgentMut.mutate(agent.name);
                              setDeletingAgent(null);
                            }}
                            className="px-2 py-0.5 rounded text-xs bg-red-500/10 text-red-400 hover:bg-red-500/20 transition-colors"
                            style={{ fontFamily: "var(--mem-font-body)" }}
                          >
                            {t("settings.agents.confirm")}
                          </button>
                          <button
                            onClick={() => setDeletingAgent(null)}
                            className="px-2 py-0.5 rounded text-xs text-[var(--mem-text-tertiary)] hover:text-[var(--mem-text)] transition-colors"
                            style={{ fontFamily: "var(--mem-font-body)" }}
                          >
                            {t("settings.agents.cancel")}
                          </button>
                        </div>
                      ) : (
                        <button
                          onClick={() => setDeletingAgent(agent.name)}
                          className="p-1 text-[var(--mem-text-tertiary)] hover:text-red-400 transition-colors"
                        >
                          <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                          </svg>
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              ))}
              {onSetupAgent && (
                <div className="px-5 py-3">
                  <button
                    onClick={onSetupAgent}
                    className="text-xs transition-colors"
                    style={{ fontFamily: "var(--mem-font-body)", color: "var(--mem-accent-indigo)" }}
                  >
                    {t("settings.agents.setupAnotherTool")}
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      </section>

      {/* ── Remote Access ─────────────────────────────────────────── */}
      <section className="mem-fade-up" style={{ animationDelay: "30ms" }}>
        <SectionHeader
          icon={
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          }
          label={t("settings.agents.remoteAccess")}
        />
        <RemoteAccessPanel mode="full" />
      </section>
      </>)}

      {/* ── Intelligence (group: intelligence) ──────────────────── */}
      {section === "intelligence" && (
      <IntelligenceSection delay={0} />
      )}

      {/* Diagnostics (group: diagnostics) */}
      {section === "diagnostics" && <DiagnosticsSection />}

      {/* Filters section hidden — legacy ambient capture, not relevant with memory-layer pivot */}

      {/* ── Privacy note — persistent footer on all groups. The settings
          sidebar also carries a shorter version in its bottom strip; this
          longer one stays here for users who prefer reading it inline. */}
      <div
        className="flex items-start gap-2.5 px-2 pt-2 mem-fade-up"
        style={{ animationDelay: "120ms" }}
      >
        <svg className="w-4 h-4 shrink-0 mt-0.5" style={{ color: "var(--mem-text-tertiary)", opacity: 0.6 }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
        </svg>
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)", opacity: 0.6, lineHeight: "1.5" }}>
          {t("settings.footer")}
        </p>
      </div>

    </div>
  );
}

function IntelligenceSection({ delay }: { delay: number }) {
  const { t } = useTranslation();
  const { isConfigured } = useApiKeyStatus();

  return (
    <section className="mem-fade-up" style={{ animationDelay: `${delay}ms` }}>
      <SectionHeader
        label={t("settings.groups.intelligence.label")}
        icon={
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
          </svg>
        }
      />
      <div className="flex flex-col gap-3">
        <ApiKeyCard />
        {!isConfigured && (
          <div className="mem-fade-up" style={{ animationDelay: `${delay + 30}ms` }}>
            <OnDeviceModelCard />
          </div>
        )}
      </div>
    </section>
  );
}
