// SPDX-License-Identifier: AGPL-3.0-only
import { useQuery } from "@tanstack/react-query";
import { getPipelineStatus, type PipelineStatusResponse } from "../../../lib/tauri";

function sortedEntries(values: Record<string, number>): [string, number][] {
  return Object.entries(values).sort(([leftKey, leftValue], [rightKey, rightValue]) => {
    if (rightValue !== leftValue) return rightValue - leftValue;
    return leftKey.localeCompare(rightKey);
  });
}

function isOldDaemonError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  const lowerMessage = message.toLowerCase();
  return (
    lowerMessage.includes("/api/debug/pipeline") &&
    (lowerMessage.includes("404") || lowerMessage.includes("not found"))
  );
}

function Divider() {
  return <div className="mx-5 border-t border-[var(--mem-border)]" style={{ opacity: 0.4 }} />;
}

function StatList({
  title,
  values,
  empty,
}: {
  title: string;
  values: Record<string, number>;
  empty: string;
}) {
  const entries = sortedEntries(values);
  return (
    <div className="px-5 py-4">
      <div className="mb-2" style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", fontWeight: 600, color: "var(--mem-text)" }}>
        {title}
      </div>
      {entries.length === 0 ? (
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-tertiary)" }}>{empty}</p>
      ) : (
        <div className="flex flex-col gap-1.5">
          {entries.map(([key, count]) => (
            <div key={key} className="flex items-center justify-between gap-3">
              <span style={{ fontFamily: "var(--mem-font-mono)", fontSize: "12px", color: "var(--mem-text-secondary)" }}>{key}</span>
              <span style={{ fontFamily: "var(--mem-font-mono)", fontSize: "12px", color: "var(--mem-text)" }}>{count}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function EntityLinking({ data }: { data: PipelineStatusResponse }) {
  const total = data.entity_linking.linked + data.entity_linking.unlinked;
  const percent = total === 0 ? null : Math.round((data.entity_linking.linked / total) * 100);
  return (
    <div className="px-5 py-4">
      <div className="mb-2" style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", fontWeight: 600, color: "var(--mem-text)" }}>
        Entity linking
      </div>
      <div className="flex items-baseline gap-3">
        <span style={{ fontFamily: "var(--mem-font-heading)", fontSize: "22px", color: "var(--mem-text)" }}>{data.entity_linking.linked}</span>
        <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)" }}>
          linked / {data.entity_linking.unlinked} unlinked
        </span>
      </div>
      {percent !== null && (
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-tertiary)", marginTop: 4 }}>
          {percent}% linked
        </p>
      )}
    </div>
  );
}

function RefineryQueue({ data }: { data: PipelineStatusResponse }) {
  return (
    <div className="px-5 py-4">
      <div className="mb-2" style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", fontWeight: 600, color: "var(--mem-text)" }}>
        Refinery queue
      </div>
      {data.refinement_queue.length === 0 ? (
        <p style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-tertiary)" }}>No pending refinery work.</p>
      ) : (
        <div className="flex flex-col gap-1.5">
          {data.refinement_queue.map((entry) => (
            <div key={`${entry.action}:${entry.status}`} className="grid grid-cols-[1fr_auto_auto] items-center gap-3">
              <span style={{ fontFamily: "var(--mem-font-mono)", fontSize: "12px", color: "var(--mem-text-secondary)" }}>{entry.action}</span>
              <span style={{ fontFamily: "var(--mem-font-mono)", fontSize: "12px", color: "var(--mem-text-tertiary)" }}>{entry.status}</span>
              <span style={{ fontFamily: "var(--mem-font-mono)", fontSize: "12px", color: "var(--mem-text)" }}>{entry.count}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function DiagnosticsError({ error }: { error: unknown }) {
  return (
    <p className="px-5 py-4" style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "#ef4444", lineHeight: "1.5" }}>
      {isOldDaemonError(error) ? "Diagnostics require a newer daemon" : "Diagnostics unavailable"}
    </p>
  );
}

export default function DiagnosticsSection() {
  const pipelineQuery = useQuery({
    queryKey: ["pipelineStatus"],
    queryFn: getPipelineStatus,
    retry: false,
  });

  return (
    <section className="mem-fade-up" style={{ animationDelay: "0ms" }}>
      <div className="flex items-center justify-between gap-3 mb-3 px-1">
        <h3 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "12px", fontWeight: 600, letterSpacing: "0.05em", color: "var(--mem-text-tertiary)", textTransform: "uppercase" as const }}>
          Pipeline Snapshot
        </h3>
        <button
          onClick={() => pipelineQuery.refetch()}
          className="px-2.5 py-1 rounded-md transition-colors hover:bg-[var(--mem-hover)]"
          style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)", border: "1px solid var(--mem-border)" }}
        >
          Refresh
        </button>
      </div>
      <div className="bg-[var(--mem-surface)] rounded-xl overflow-hidden border border-[var(--mem-border)]">
        {pipelineQuery.isLoading && (
          <p className="px-5 py-4" style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "var(--mem-text-secondary)" }}>
            Loading diagnostics...
          </p>
        )}
        {pipelineQuery.isError && <DiagnosticsError error={pipelineQuery.error} />}
        {pipelineQuery.data && (
          <>
            <StatList title="Enrichment" values={pipelineQuery.data.enrichment} empty="No enrichment rows." />
            <Divider />
            <EntityLinking data={pipelineQuery.data} />
            <Divider />
            <RefineryQueue data={pipelineQuery.data} />
            <Divider />
            <div className="px-5 py-4">
              <div className="mb-1" style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", fontWeight: 600, color: "var(--mem-text)" }}>Recaps</div>
              <span style={{ fontFamily: "var(--mem-font-heading)", fontSize: "22px", color: "var(--mem-text)" }}>{pipelineQuery.data.recaps}</span>
            </div>
            <Divider />
            <StatList title="Memory types" values={pipelineQuery.data.types} empty="No memory type rows." />
            <Divider />
            <StatList title="Quality" values={pipelineQuery.data.quality} empty="No quality rows." />
          </>
        )}
      </div>
    </section>
  );
}
