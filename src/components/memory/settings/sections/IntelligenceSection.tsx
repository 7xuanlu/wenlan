// SPDX-License-Identifier: AGPL-3.0-only
import { useTranslation } from "react-i18next";
import ActiveIntelligenceStrip from "../../../intelligence/ActiveIntelligenceStrip";
import AnyProviderCard from "../../../intelligence/AnyProviderCard";
import { ApiKeyCard, OnDeviceModelCard, useApiKeyStatus } from "../../../intelligence/IntelligenceSetup";
import { SectionHeader } from "../primitives";

export default function IntelligenceSection({ delay }: { delay: number }) {
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
        <ActiveIntelligenceStrip />
        <ApiKeyCard />
        {!isConfigured && (
          <div className="mem-fade-up" style={{ animationDelay: `${delay + 30}ms` }}>
            <OnDeviceModelCard />
          </div>
        )}
        <AnyProviderCard />
      </div>
    </section>
  );
}
