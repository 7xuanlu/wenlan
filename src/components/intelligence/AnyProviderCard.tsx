// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useMemo, useRef, useState, type CSSProperties } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import {
  getExternalLlm,
  setExternalLlm,
  testExternalLlm,
  listExternalModels,
  getExternalLlmKeyConfigured,
} from "../../lib/tauri";
import { useDaemonVersion } from "../../hooks/useDaemonVersion";
import { useApiKeyStatus } from "./IntelligenceSetup";
import {
  PROVIDER_PRESETS,
  presetForEndpoint,
  keyPrefixMismatch,
  type PresetGroup,
} from "./providerPresets";

interface Props {
  /** Filter the preset picker (wizard: cloud-only / local-only). */
  groups?: PresetGroup[];
  initialPresetId?: string;
  hidePresetPicker?: boolean;
}

// The preset table is the single source of truth for local endpoints — look
// them up by id rather than restating the URLs here, so a future table edit
// can't drift the probed endpoint away from the host shown in the UI.
function presetEndpoint(id: string): string {
  const preset = PROVIDER_PRESETS.find((p) => p.id === id);
  if (!preset) throw new Error(`Unknown preset id: ${id}`);
  return preset.endpoint;
}
const OLLAMA_ENDPOINT = presetEndpoint("ollama");
const LMSTUDIO_ENDPOINT = presetEndpoint("lmstudio");
const hostOf = (ep: string) => ep.replace(/^https?:\/\//, "").replace(/\/.*$/, "");
const localLabel = (name: string) => name.replace(/\s*\(local\)$/i, "");

const fieldStyle: CSSProperties = {
  width: "100%",
  padding: "8px 10px",
  borderRadius: "8px",
  border: "1px solid var(--mem-border)",
  backgroundColor: "var(--mem-bg)",
  color: "var(--mem-text)",
  fontFamily: "var(--mem-font-mono)",
  fontSize: "12px",
};

const labelStyle: CSSProperties = {
  fontFamily: "var(--mem-font-body)",
  fontSize: "12px",
  fontWeight: 500,
  color: "var(--mem-text-secondary)",
};

export default function AnyProviderCard({ groups, initialPresetId, hidePresetPicker }: Props) {
  const { t } = useTranslation();
  const { supportsExternalKey, supportsHotSwap } = useDaemonVersion();
  const anthropic = useApiKeyStatus();
  const queryClient = useQueryClient();
  const keyHintId = useId();

  const presets = useMemo(
    () =>
      PROVIDER_PRESETS.filter(
        (p) => !groups || groups.includes(p.group) || p.group === "custom"
      ),
    [groups]
  );

  const isLocalOnly = groups?.length === 1 && groups[0] === "local";

  const [presetId, setPresetId] = useState(initialPresetId ?? presets[0].id);
  const preset = presets.find((p) => p.id === presetId) ?? presets[presets.length - 1];
  const [endpoint, setEndpoint] = useState(preset.endpoint);
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [testState, setTestState] = useState<
    | { kind: "idle" }
    | { kind: "testing" }
    | { kind: "ok"; response: string }
    | { kind: "error"; message: string }
  >({ kind: "idle" });
  const [saveState, setSaveState] = useState<
    "idle" | "saving" | "applied" | "restart" | `error:${string}`
  >("idle");

  // Prefill from the daemon's current external slot.
  const { data: current } = useQuery({ queryKey: ["external-llm"], queryFn: getExternalLlm });
  const { data: keyConfigured } = useQuery({
    queryKey: ["external-llm-key-configured"],
    queryFn: getExternalLlmKeyConfigured,
  });
  useEffect(() => {
    if (!current) return;
    const [savedEndpoint, savedModel] = current;
    if (savedEndpoint) {
      setEndpoint(savedEndpoint);
      setPresetId(presetForEndpoint(savedEndpoint).id);
    }
    if (savedModel) setModel(savedModel);
    // Run once when the saved config arrives; later edits are user-driven.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [current]);

  // Keyed cloud presets stay dark below daemon 0.13 (spec §8).
  const lockedByVersion = preset.keyRequired && !supportsExternalKey;

  const trimmedEndpoint = endpoint.trim().replace(/\/+$/, "");
  const endpointValid = /^https?:\/\//.test(trimmedEndpoint);

  // §9.2: probe BOTH local servers on the Local-server pane mount.
  const ollamaProbe = useQuery({
    queryKey: ["local-probe", OLLAMA_ENDPOINT],
    queryFn: () => listExternalModels(OLLAMA_ENDPOINT, null),
    enabled: !!isLocalOnly,
    retry: false,
    staleTime: 30_000,
  });
  const lmStudioProbe = useQuery({
    queryKey: ["local-probe", LMSTUDIO_ENDPOINT],
    queryFn: () => listExternalModels(LMSTUDIO_ENDPOINT, null),
    enabled: !!isLocalOnly,
    retry: false,
    staleTime: 30_000,
  });
  const probeFor = (id: string) =>
    id === "ollama" ? ollamaProbe : id === "lmstudio" ? lmStudioProbe : null;
  // A probe is only "the" probe for the selected preset while the live
  // (trimmed) endpoint still matches that preset's endpoint — once the user
  // hand-edits Endpoint away from e.g. Ollama's default, the probed models
  // belong to a different server than the one now in the field, so the
  // association must drop (Task 5 review finding: never show a dropdown of
  // the wrong server's models, and never leave the edited endpoint with no
  // discovery at all).
  const probedEndpointFor = (id: string) =>
    id === "ollama" ? OLLAMA_ENDPOINT : id === "lmstudio" ? LMSTUDIO_ENDPOINT : null;
  const selectedProbe =
    probedEndpointFor(presetId) === trimmedEndpoint ? probeFor(presetId) : null;

  // Model auto-discovery; silent fallback to free text on failure (spec §1).
  // Ollama/LM Studio already have a dedicated probe above — reusing this
  // query for them would duplicate the fetch, so it stays off for those two.
  // A hand-typed or "Custom…" local endpoint has no dedicated probe, so this
  // query is the only discovery source for it — kept enabled even while
  // `isLocalOnly` is true whenever the selected preset isn't one of the two
  // probed ones (§9.2: no discovery hole for unprobed local endpoints).
  const discovery = useQuery({
    queryKey: ["external-models", trimmedEndpoint, apiKey],
    queryFn: () => listExternalModels(trimmedEndpoint, apiKey || null),
    enabled: endpointValid && !lockedByVersion && (!isLocalOnly || !selectedProbe),
    retry: false,
    staleTime: 30_000,
  });
  const models = discovery.data ?? [];

  // §9.2 parity: the wizard local pane reads the fixed-endpoint probes; the
  // settings card (all groups) reuses the generic discovery query when a
  // local preset is selected. `localQuery` drives the chip and the <select>.
  //
  // Task 5b review finding: locality must be decided by the *live* endpoint
  // (`trimmedEndpoint`, the same value `discovery` actually probes), never
  // by the merely-selected preset id — otherwise picking "Ollama (local)"
  // and then hand-editing Endpoint to a cloud host renders a chip claiming
  // "Connected to Ollama" sourced from that cloud provider's models. This
  // mirrors the wizard's `selectedProbe` invariant above (probe association
  // drops the moment the endpoint no longer matches).
  const endpointPreset = presetForEndpoint(trimmedEndpoint);
  const isLocalEndpoint = endpointPreset.group === "local";
  const localQuery = isLocalOnly ? selectedProbe : isLocalEndpoint ? discovery : null;
  const localQueryModels = localQuery?.data ?? [];
  // The chip's vendor name must come from whichever preset the live endpoint
  // resolves to (settings card) — the wizard pane keeps the selected preset
  // and the endpoint in sync via its pills, so it can use `preset` directly.
  const chipPreset = isLocalOnly ? preset : endpointPreset;

  // Auto-select the single responder, once both probes have settled.
  const autoSelectedRef = useRef(false);
  useEffect(() => {
    if (!isLocalOnly || autoSelectedRef.current) return;
    if (ollamaProbe.isLoading || lmStudioProbe.isLoading) return;
    // Wait for the saved-config query to settle before latching a decision —
    // otherwise a fast probe can auto-select while `current` is still
    // undefined, and the prefill effect then re-applies the saved endpoint
    // right after, causing a visible double-set of the endpoint field.
    if (current === undefined) return;
    autoSelectedRef.current = true; // decide exactly once
    // Respect a previously-saved local endpoint over auto-selection.
    if (current[0]) return;
    const up = [
      ["ollama", ollamaProbe.isSuccess] as const,
      ["lmstudio", lmStudioProbe.isSuccess] as const,
    ].filter(([, ok]) => ok);
    if (up.length === 1) {
      const id = up[0][0];
      setPresetId(id);
      setEndpoint(id === "ollama" ? OLLAMA_ENDPOINT : LMSTUDIO_ENDPOINT);
    }
  }, [
    isLocalOnly,
    ollamaProbe.isLoading,
    ollamaProbe.isSuccess,
    lmStudioProbe.isLoading,
    lmStudioProbe.isSuccess,
    current,
  ]);

  const selectPreset = (id: string) => {
    setPresetId(id);
    const next = PROVIDER_PRESETS.find((p) => p.id === id);
    if (next && next.endpoint) setEndpoint(next.endpoint);
    if (next && !next.endpoint) setEndpoint("");
    setModel("");
    setApiKey("");
    setTestState({ kind: "idle" });
    setSaveState("idle");
  };

  const keyToSend = (): string | undefined =>
    supportsExternalKey && apiKey !== "" ? apiKey : undefined;

  const handleTest = async () => {
    setTestState({ kind: "testing" });
    try {
      const resp = await testExternalLlm(trimmedEndpoint, model, keyToSend() ?? null);
      setTestState({ kind: "ok", response: resp.response });
    } catch (err) {
      // Verbatim daemon error (spec: Error handling).
      setTestState({ kind: "error", message: err instanceof Error ? err.message : String(err) });
    }
  };

  const handleSave = async () => {
    setSaveState("saving");
    try {
      await setExternalLlm(trimmedEndpoint, model, keyToSend());
      setSaveState(supportsHotSwap ? "applied" : "restart");
      // The daemon may hot-load this config immediately (spec §7.6), so the
      // strip's status queries must refresh alongside our own prefill query —
      // otherwise ActiveIntelligenceStrip keeps showing stale state.
      queryClient.invalidateQueries({ queryKey: ["setup-status"] });
      queryClient.invalidateQueries({ queryKey: ["external-llm"] });
      queryClient.invalidateQueries({ queryKey: ["external-llm-key-configured"] });
    } catch (err) {
      setSaveState(`error:${err instanceof Error ? err.message : String(err)}`);
    }
  };

  const canAct = endpointValid && model.trim() !== "" && !lockedByVersion;

  return (
    <div
      className="rounded-xl p-4 flex flex-col"
      style={{ border: "1px solid var(--mem-border)", backgroundColor: "var(--mem-surface)", gap: "12px" }}
    >
      <div>
        <h3 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "15px", fontWeight: 500, color: "var(--mem-text)" }}>
          {t("externalProvider.title")}
        </h3>
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", lineHeight: 1.5, marginTop: "4px" }}>
          {t("externalProvider.description")}
        </p>
      </div>

      {anthropic.isConfigured && (
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-accent-amber)", lineHeight: 1.5 }}>
          {t("externalProvider.anthropicPrecedence")}
        </p>
      )}

      {isLocalOnly ? (
        <div
          className="flex flex-wrap gap-2"
          role="group"
          aria-label={t("externalProvider.presetLabel")}
        >
          {presets.map((p) => {
            const probe = probeFor(p.id);
            const status = !probe
              ? null
              : probe.isLoading
              ? "probing"
              : probe.isSuccess
              ? "connected"
              : "notDetected";
            const dot = status === "connected" ? "●" : status === "notDetected" ? "○" : "…";
            const selected = p.id === presetId;
            return (
              <button
                key={p.id}
                type="button"
                onClick={() => selectPreset(p.id)}
                aria-pressed={selected}
                className="rounded-full px-3 py-1.5 text-xs"
                style={{
                  border: `1px solid ${selected ? "var(--mem-accent-indigo)" : "var(--mem-border)"}`,
                  backgroundColor: selected ? "var(--mem-accent-indigo)" : "var(--mem-surface)",
                  color: selected ? "white" : "var(--mem-text)",
                  fontFamily: "var(--mem-font-body)",
                }}
              >
                {probe && (
                  <span
                    aria-hidden="true"
                    style={{
                      marginRight: "6px",
                      color:
                        status === "connected" && !selected
                          ? "var(--mem-accent-sage)"
                          : "inherit",
                    }}
                  >
                    {dot}
                  </span>
                )}
                {localLabel(p.name)}
              </button>
            );
          })}
        </div>
      ) : !hidePresetPicker ? (
        <label className="flex flex-col gap-1">
          <span style={labelStyle}>{t("externalProvider.presetLabel")}</span>
          <select value={presetId} onChange={(e) => selectPreset(e.target.value)} style={fieldStyle}>
            {presets.map((p) => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
        </label>
      ) : null}

      {localQuery && (isLocalOnly || endpointValid) && (
        <p
          role="status"
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "12px",
            lineHeight: 1.5,
            color: localQuery.isSuccess
              ? "var(--mem-accent-sage)"
              : "var(--mem-text-secondary)",
          }}
        >
          {localQuery.isLoading
            ? t("externalProvider.localProbing", { name: localLabel(chipPreset.name) })
            : localQuery.isSuccess
            ? t("externalProvider.localConnectedChip", {
                name: localLabel(chipPreset.name),
                modelCount: localQueryModels.length,
              })
            : t("externalProvider.localNotDetectedChip", {
                name: localLabel(chipPreset.name),
                host: hostOf(isLocalOnly ? chipPreset.endpoint : trimmedEndpoint),
              })}
        </p>
      )}

      {lockedByVersion ? (
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", lineHeight: 1.5 }}>
          {t("externalProvider.keyNeedsUpgrade")}
        </p>
      ) : (
        <>
          <label className="flex flex-col gap-1">
            <span style={labelStyle}>{t("externalProvider.endpointLabel")}</span>
            <input type="text" value={endpoint} onChange={(e) => setEndpoint(e.target.value)} style={fieldStyle} />
          </label>

          <label className="flex flex-col gap-1">
            <span style={labelStyle}>{t("externalProvider.modelLabel")}</span>
            {localQuery && localQueryModels.length >= 1 ? (
              <select value={model} onChange={(e) => setModel(e.target.value)} style={fieldStyle}>
                <option value="">{t("externalProvider.modelSelectPlaceholder")}</option>
                {/* A saved model that's no longer among the discovered ids
                    (e.g. removed from Ollama since it was saved) must still
                    render as the visibly-selected value — never a blank
                    select while Save/Test remain enabled on a value the user
                    can't see. */}
                {model !== "" && !localQueryModels.includes(model) && (
                  <option value={model}>{model}</option>
                )}
                {localQueryModels.map((m) => (
                  <option key={m} value={m}>{m}</option>
                ))}
              </select>
            ) : (
              <>
                <input
                  type="text"
                  value={model}
                  onChange={(e) => setModel(e.target.value)}
                  placeholder={t("externalProvider.modelPlaceholder")}
                  list="any-provider-models"
                  style={fieldStyle}
                />
                <datalist id="any-provider-models">
                  {models.map((m) => (
                    <option key={m} value={m} />
                  ))}
                </datalist>
              </>
            )}
          </label>
          {(discovery.isError || localQuery?.isError) && (
            <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}>
              {t("externalProvider.modelDiscoveryFailed")}
            </span>
          )}

          {supportsExternalKey && (
            <div className="flex flex-col gap-1">
              <label className="flex flex-col gap-1">
                <span style={labelStyle}>{t("externalProvider.apiKeyLabel")}</span>
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder={
                    keyConfigured
                      ? t("externalProvider.apiKeyConfiguredPlaceholder")
                      : (preset.keyPlaceholder ?? "")
                  }
                  style={fieldStyle}
                  aria-describedby={keyPrefixMismatch(preset, apiKey) ? keyHintId : undefined}
                />
              </label>
              {keyPrefixMismatch(preset, apiKey) && (
                <span
                  id={keyHintId}
                  style={{
                    fontFamily: "var(--mem-font-body)",
                    fontSize: "11px",
                    color: "var(--mem-accent-amber)",
                  }}
                >
                  {t("externalProvider.keyHint", {
                    vendor: preset.name,
                    prefix: (preset.keyPrefixes ?? []).join(" or "),
                  })}
                </span>
              )}
              {preset.getKeyUrl && (
                <button
                  type="button"
                  onClick={() => shellOpen(preset.getKeyUrl!)}
                  className="self-start text-xs"
                  style={{
                    fontFamily: "var(--mem-font-body)",
                    color: "var(--mem-accent-indigo)",
                    background: "none",
                    border: "none",
                    padding: 0,
                    cursor: "pointer",
                  }}
                >
                  {t("externalProvider.getKeyLink")}
                </button>
              )}
            </div>
          )}
        </>
      )}

      <div className="flex items-center gap-2">
        <button
          onClick={handleTest}
          disabled={!canAct || testState.kind === "testing"}
          className="rounded-md px-3 py-1.5 text-xs disabled:opacity-50"
          style={{ border: "1px solid var(--mem-border)", color: "var(--mem-text)", fontFamily: "var(--mem-font-body)" }}
        >
          {testState.kind === "testing" ? t("externalProvider.testing") : t("externalProvider.test")}
        </button>
        <button
          onClick={handleSave}
          disabled={!canAct || saveState === "saving"}
          className="rounded-md px-3 py-1.5 text-xs font-medium disabled:opacity-50"
          style={{ backgroundColor: "var(--mem-accent-indigo)", color: "white", fontFamily: "var(--mem-font-body)" }}
        >
          {saveState === "saving" ? t("externalProvider.saving") : t("externalProvider.save")}
        </button>
      </div>

      {testState.kind === "ok" && (
        <p style={{ fontFamily: "var(--mem-font-mono)", fontSize: "11px", color: "var(--mem-text-secondary)" }}>
          {t("externalProvider.testOk", { response: testState.response })}
        </p>
      )}
      {testState.kind === "error" && (
        <p className="text-red-500" style={{ fontFamily: "var(--mem-font-mono)", fontSize: "11px" }}>
          {testState.message}
        </p>
      )}
      {saveState === "applied" && (
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-accent-sage)" }}>
          {t("externalProvider.savedApplied")}
        </p>
      )}
      {saveState === "restart" && (
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)" }}>
          {t("externalProvider.savedRestart")}
        </p>
      )}
      {saveState.startsWith("error:") && (
        <p className="text-red-500" style={{ fontFamily: "var(--mem-font-mono)", fontSize: "11px" }}>
          {saveState.slice("error:".length)}
        </p>
      )}
    </div>
  );
}
