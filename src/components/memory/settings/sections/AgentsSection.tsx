// SPDX-License-Identifier: AGPL-3.0-only
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  listAgents,
  updateAgent,
  deleteAgent,
  detectMcpClients,
} from "../../../../lib/tauri";
import { describeTrustLevel, resolveAgentDisplayName, TRUST_LEVELS } from "../../../../lib/agents";
import { RemoteAccessPanel } from "../../RemoteAccessPanel";
import { SectionHeader, Toggle } from "../primitives";
import WebPlatformCards from "../../../connect/WebPlatformCards";
import ClientSetupList from "../../../connect/ClientSetupList";

export default function AgentsSection({ onSetupAgent }: { onSetupAgent?: () => void }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
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

  return (
    <>
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

      {/* ── Web — Claude.ai & ChatGPT ─────────────────────────────── */}
      <section className="mem-fade-up" style={{ animationDelay: "45ms" }}>
        <SectionHeader
          icon={
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 21a9.004 9.004 0 008.716-6.747M12 21a9.004 9.004 0 01-8.716-6.747M12 21c2.485 0 4.5-4.03 4.5-9S14.485 3 12 3m0 18c-2.485 0-4.5-4.03-4.5-9S9.515 3 12 3m0 0a8.997 8.997 0 017.843 4.582M12 3a8.997 8.997 0 00-7.843 4.582m15.686 0A11.953 11.953 0 0112 10.5c-2.998 0-5.74-1.1-7.843-2.918m15.686 0A8.959 8.959 0 0121 12c0 .778-.099 1.533-.284 2.253m0 0A11.953 11.953 0 0112 13.5c-2.998 0-5.74-1.1-7.843-2.918m0 0A8.959 8.959 0 003 12c0-.778.099-1.533.284-2.253" />
            </svg>
          }
          label={t("connectMatrix.webTitle")}
        />
        <WebPlatformCards />
      </section>

      {/* ── Apps & CLIs ───────────────────────────────────────────── */}
      <section className="mem-fade-up" style={{ animationDelay: "60ms" }}>
        <SectionHeader
          icon={
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M6.75 7.5l3 2.25-3 2.25m4.5 0h3m-9-9h13.5c.621 0 1.125.504 1.125 1.125v9.75c0 .621-.504 1.125-1.125 1.125H4.5A1.125 1.125 0 013.375 16.5v-9.75c0-.621.504-1.125 1.125-1.125z" />
            </svg>
          }
          label={t("connectMatrix.appsTitle")}
        />
        <ClientSetupList />
      </section>
    </>
  );
}
