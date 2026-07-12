// SPDX-License-Identifier: AGPL-3.0-only
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { getSetupStatus, getExternalLlm } from "../../lib/tauri";
import { useDaemonVersion } from "../../hooks/useDaemonVersion";

/** One-line honest status of the daemon priority chain (spec §1, council
 *  change c): "serving" only from daemon runtime state, never from config. */
export default function ActiveIntelligenceStrip() {
  const { t } = useTranslation();
  const { supportsHotSwap } = useDaemonVersion(); // ≥0.13 ⇔ setup status carries external_llm
  const { data: status } = useQuery({ queryKey: ["setup-status"], queryFn: getSetupStatus });
  const { data: external } = useQuery({ queryKey: ["external-llm"], queryFn: getExternalLlm });

  const externalConfigured = Boolean(external?.[0]);

  // Top of the chain first: Anthropic → external → on-device → basic.
  let topLine: string;
  if (status?.anthropic_key_configured) {
    topLine = t("intelligenceStrip.servingAnthropic");
  } else if (supportsHotSwap && status?.external_llm) {
    // Daemon ≥0.13 reports runtime state (spec §7.6).
    topLine = status.external_llm.loaded
      ? t("intelligenceStrip.servingExternal")
      : status.external_llm.configured
        ? t("intelligenceStrip.externalRestartPending")
        : status?.local_model_loaded
          ? t("intelligenceStrip.servingOnDevice")
          : t("intelligenceStrip.servingBasic");
  } else if (externalConfigured) {
    // 0.12: config is all we can see — never claim serving.
    topLine = t("intelligenceStrip.externalUnverified");
  } else if (status?.local_model_loaded) {
    topLine = t("intelligenceStrip.servingOnDevice");
  } else {
    topLine = t("intelligenceStrip.servingBasic");
  }

  return (
    <div
      className="rounded-lg px-4 py-3 flex flex-col"
      style={{ backgroundColor: "var(--mem-hover)", border: "1px solid var(--mem-border)", gap: "2px" }}
    >
      <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", fontWeight: 500, color: "var(--mem-text)" }}>
        {topLine}
      </span>
      <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}>
        {t("intelligenceStrip.chain")}
      </span>
    </div>
  );
}
