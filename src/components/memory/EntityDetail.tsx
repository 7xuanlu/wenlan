// SPDX-License-Identifier: AGPL-3.0-only
import { useMemo, useRef, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  getEntityDetail,
  updateObservation,
  deleteObservation,
  addObservation,
  confirmObservation,
  confirmEntity,
  deleteEntity,
  search,
  FACET_COLORS,
  type Observation,
  type RelationWithEntity,
} from "../../lib/tauri";
import { MetadataRow, RailPanelTitle } from "./MemoryDetailPrimitives";

interface EntityDetailProps {
  entityId: string;
  onBack: () => void;
  onEntityClick: (entityId: string) => void;
  onMemoryClick?: (sourceId: string) => void;
}

interface GraphNeighbor {
  entityId: string;
  name: string;
  entityType: string;
  verbs: string[];
  direction: "incoming" | "outgoing";
}

const GRAPH_NODE_CAP = 8;

function timeAgo(ts: number, locale: string): string {
  const diff = Math.floor(Date.now() / 1000 - ts);
  const formatter = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  if (diff < 60) return formatter.format(-diff, "second");
  if (diff < 3600) return formatter.format(-Math.floor(diff / 60), "minute");
  if (diff < 86400) return formatter.format(-Math.floor(diff / 3600), "hour");
  if (diff < 604800) return formatter.format(-Math.floor(diff / 86400), "day");
  return new Date(ts * 1000).toLocaleDateString(locale);
}

function absoluteDate(ts: number): string {
  return new Date(ts * 1000).toLocaleString();
}

function monogram(name: string): string {
  return name
    .split(/\s+/)
    .map((w) => w[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
}

interface EntityGraphProps {
  name: string;
  neighbors: GraphNeighbor[];
  hiddenCount: number;
  onEntityClick: (entityId: string) => void;
}

/** One-hop connection map: this entity centered, incoming relations on the
 *  left, outgoing on the right. DOM buttons over an SVG edge layer so every
 *  neighbor stays keyboard-reachable (unlike the canvas ConstellationMap). */
function EntityGraph({ name, neighbors, hiddenCount, onEntityClick }: EntityGraphProps) {
  const { t } = useTranslation();
  const incoming = neighbors.filter((n) => n.direction === "incoming");
  const outgoing = neighbors.filter((n) => n.direction === "outgoing");
  const maxSide = Math.max(incoming.length, outgoing.length);
  const height = Math.min(280, 128 + maxSide * 30);

  const place = (list: GraphNeighbor[], side: "left" | "right") =>
    list.map((n, i) => {
      const y = ((i + 1) / (list.length + 1)) * 100;
      const spread = (i % 2) * 6;
      const x = side === "right" ? 76 - spread : 24 + spread;
      return { ...n, x, y };
    });
  const placed = [...place(incoming, "left"), ...place(outgoing, "right")];

  return (
    <div
      className="entity-graph"
      style={{ height }}
      role="group"
      aria-label={t("entityDetail.graphLabel", { name })}
    >
      <svg
        className="entity-graph-edges"
        viewBox="0 0 100 100"
        preserveAspectRatio="none"
        aria-hidden="true"
      >
        {placed.map((n) => (
          <line
            key={`edge-${n.entityId}-${n.direction}`}
            x1="50"
            y1="50"
            x2={n.x}
            y2={n.y}
            vectorEffect="non-scaling-stroke"
          />
        ))}
      </svg>
      {incoming.length > 0 && (
        <span className="entity-graph-caption is-left" aria-hidden="true">
          ← {t("entityDetail.incoming")}
        </span>
      )}
      {outgoing.length > 0 && (
        <span className="entity-graph-caption is-right" aria-hidden="true">
          {t("entityDetail.outgoing")} →
        </span>
      )}
      {placed.map((n) => (
        <span
          key={`verb-${n.entityId}-${n.direction}`}
          className="entity-graph-verb"
          style={{ left: `${50 + (n.x - 50) * 0.55}%`, top: `${50 + (n.y - 50) * 0.55}%` }}
          aria-hidden="true"
        >
          {n.verbs[0]}
          {n.verbs.length > 1 ? ` +${n.verbs.length - 1}` : ""}
        </span>
      ))}
      <div className="entity-graph-center" aria-hidden="true">
        <span className="entity-graph-center-dot" />
        <span className="entity-graph-center-name">{name}</span>
      </div>
      {placed.map((n) => {
        const isIncoming = n.direction === "incoming";
        const dirLabel = t(isIncoming ? "entityDetail.incoming" : "entityDetail.outgoing");
        return (
        <button
          key={`node-${n.entityId}-${n.direction}`}
          type="button"
          className={`entity-graph-node ${isIncoming ? "is-left" : "is-right"}`}
          style={{ left: `${n.x}%`, top: `${n.y}%` }}
          onClick={() => onEntityClick(n.entityId)}
          title={`${n.verbs.join(", ")} — ${n.name}`}
          aria-label={`${n.name} (${n.entityType}) · ${dirLabel} · ${n.verbs.join(", ")}`}
        >
          <span className="entity-graph-node-dot" aria-hidden="true" />
          <span className="entity-graph-node-text">
            <span className="entity-graph-node-name">{n.name}</span>
            <span className="entity-graph-node-type">{n.entityType}</span>
          </span>
        </button>
        );
      })}
      {hiddenCount > 0 && (
        <span className="entity-graph-more">
          {t("entityDetail.moreConnections", { count: hiddenCount })}
        </span>
      )}
    </div>
  );
}

export default function EntityDetail({ entityId, onBack, onEntityClick, onMemoryClick }: EntityDetailProps) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const [editingObs, setEditingObs] = useState<string | null>(null);
  const [editContent, setEditContent] = useState("");
  const [newObs, setNewObs] = useState("");
  const [showAddForm, setShowAddForm] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  // Set when Escape cancels an edit, so the unmount blur doesn't save the draft
  const cancelEditRef = useRef(false);

  const locale = i18n.resolvedLanguage ?? i18n.language;

  const { data: detail, isError, refetch } = useQuery({
    queryKey: ["entityDetail", entityId],
    queryFn: () => getEntityDetail(entityId),
    refetchInterval: 5000,
  });

  const invalidate = () => {
    queryClient.invalidateQueries({ queryKey: ["entityDetail", entityId] });
    queryClient.invalidateQueries({ queryKey: ["entities"] });
  };

  const updateMutation = useMutation({
    mutationFn: ({ id, content }: { id: string; content: string }) =>
      updateObservation(id, content),
    onSuccess: invalidate,
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteObservation(id),
    onSuccess: invalidate,
  });

  const addMutation = useMutation({
    mutationFn: (content: string) => addObservation(entityId, content, "human", 1.0),
    onSuccess: () => {
      setNewObs("");
      setShowAddForm(false);
      invalidate();
    },
  });

  const confirmObsMutation = useMutation({
    mutationFn: ({ id, confirmed }: { id: string; confirmed: boolean }) =>
      confirmObservation(id, confirmed),
    onSuccess: invalidate,
  });

  const confirmEntityMutation = useMutation({
    mutationFn: (confirmed: boolean) => confirmEntity(entityId, confirmed),
    onSuccess: invalidate,
  });

  const deleteEntityMutation = useMutation({
    mutationFn: () => deleteEntity(entityId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["entities"] });
      queryClient.invalidateQueries({ queryKey: ["constellation-entities"] });
      onBack();
    },
  });

  // Appears in: recall-matched memories. Heuristic (name search + score
  // filter) until the daemon exposes memory_entities on the detail route.
  const { data: linkedMemories = [] } = useQuery({
    queryKey: ["entity-linked-memories", entityId, detail?.entity?.name],
    queryFn: async () => {
      if (!detail?.entity?.name) return [];
      const results = await search(detail.entity.name, 10, "memory");
      return results.filter((r) => r.entity_id === entityId || r.score > 0.7).slice(0, 8);
    },
    enabled: !!detail?.entity?.name,
    staleTime: 30000,
  });

  // Deduplicate observations by content (safety net for DB-level dupes)
  const observations = useMemo(
    () =>
      (detail?.observations ?? []).filter(
        (obs, i, arr) =>
          arr.findIndex((o) => o.content.toLowerCase() === obs.content.toLowerCase()) === i,
      ),
    [detail?.observations],
  );

  // Ledger shows raw relation records (curation needs to see duplicates);
  // only the graph aggregates — its neighbor Map dedupes per (entity, direction).
  const relations = useMemo(() => detail?.relations ?? [], [detail?.relations]);

  const sortedRelations = useMemo(
    () =>
      [...relations].sort((a, b) =>
        a.direction === b.direction
          ? a.relation_type.localeCompare(b.relation_type)
          : a.direction === "outgoing"
            ? -1
            : 1,
      ),
    [relations],
  );

  // One graph node per (neighbor, direction); verbs collapse onto the edge.
  const neighbors = useMemo(() => {
    const map = new Map<string, GraphNeighbor>();
    for (const rel of relations) {
      const direction = rel.direction === "incoming" ? ("incoming" as const) : ("outgoing" as const);
      const key = `${rel.entity_id}:${direction}`;
      const existing = map.get(key);
      if (existing) {
        if (!existing.verbs.includes(rel.relation_type)) existing.verbs.push(rel.relation_type);
      } else {
        map.set(key, {
          entityId: rel.entity_id,
          name: rel.entity_name,
          entityType: rel.entity_type,
          verbs: [rel.relation_type],
          direction,
        });
      }
    }
    return Array.from(map.values());
  }, [relations]);

  if (!detail) {
    return (
      <main className="memory-detail-dossier" aria-label={t("entityDetail.dossierLabel")}>
        <header className="entity-dossier-header">
          <button
            type="button"
            onClick={onBack}
            className="memory-detail-back"
            aria-label={t("entityDetail.back")}
            title={t("entityDetail.back")}
          >
            <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M19 12H5M12 19l-7-7 7-7" /></svg>
          </button>
        </header>
        <div className="entity-detail-status" role="status">
          {isError ? (
            <>
              <p className="entity-empty">{t("entityDetail.loadError")}</p>
              <button type="button" className="memory-detail-text-button" onClick={() => refetch()}>
                {t("entityDetail.retry")}
              </button>
            </>
          ) : (
            <p className="entity-empty">{t("entityDetail.loading")}</p>
          )}
        </div>
      </main>
    );
  }

  const { entity } = detail;
  const space = entity.space ?? entity.domain;
  // Cap per direction so one lopsided side can't crowd the other out of the map
  const incomingAll = neighbors.filter((n) => n.direction === "incoming");
  const outgoingAll = neighbors.filter((n) => n.direction === "outgoing");
  const outgoingShown = Math.min(outgoingAll.length, GRAPH_NODE_CAP - Math.min(incomingAll.length, Math.ceil(GRAPH_NODE_CAP / 2)));
  const shownNeighbors = [
    ...incomingAll.slice(0, GRAPH_NODE_CAP - outgoingShown),
    ...outgoingAll.slice(0, outgoingShown),
  ];
  const hiddenCount = neighbors.length - shownNeighbors.length;

  const startEdit = (obs: Observation) => {
    setEditingObs(obs.id);
    setEditContent(obs.content);
  };

  const saveEdit = (id: string) => {
    if (updateMutation.isPending) return;
    const trimmed = editContent.trim();
    if (!trimmed || trimmed === observations.find((o) => o.id === id)?.content) {
      setEditingObs(null);
      return;
    }
    // Stay in edit mode until the write lands, so a failure keeps the draft
    updateMutation.mutate({ id, content: trimmed }, { onSuccess: () => setEditingObs(null) });
  };

  const relationRow = (rel: RelationWithEntity) => (
    <button
      key={rel.id}
      type="button"
      className="entity-relation-row"
      onClick={() => onEntityClick(rel.entity_id)}
    >
      <span className="entity-relation-verb">
        {rel.direction === "incoming" ? `← ${rel.relation_type}` : `${rel.relation_type} →`}
      </span>
      <span className="entity-relation-name">{rel.entity_name}</span>
      <span className="entity-relation-type">{rel.entity_type}</span>
    </button>
  );

  return (
    <main className="memory-detail-dossier" aria-label={t("entityDetail.dossierLabel")}>
      <header className="entity-dossier-header">
        <button
          type="button"
          onClick={onBack}
          className="memory-detail-back"
          aria-label={t("entityDetail.back")}
          title={t("entityDetail.back")}
        >
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M19 12H5M12 19l-7-7 7-7" /></svg>
        </button>
        <div className="entity-dossier-hero">
          <div className="entity-dossier-hero-row">
            <div className="entity-detail-head">
              <div className="entity-detail-seal" aria-hidden="true">
                {monogram(entity.name)}
              </div>
              <div className="flex-1 min-w-0">
                <h2 className="memory-detail-title">{entity.name}</h2>
                <span className="memory-detail-timestamp" title={absoluteDate(entity.updated_at)}>
                  {entity.entity_type}
                  {space ? ` · ${space}` : ""}
                  {` · ${timeAgo(entity.updated_at, locale)}`}
                </span>
              </div>
            </div>
            <div className="memory-detail-actions">
              {(confirmEntityMutation.isError || deleteEntityMutation.isError) && (
                <span className="entity-error" role="alert">{t("entityDetail.saveError")}</span>
              )}
              <button
                type="button"
                disabled={confirmEntityMutation.isPending}
                onClick={() => confirmEntityMutation.mutate(!entity.confirmed)}
                className={`memory-detail-chip entity-status-chip ${entity.confirmed ? "success" : "warning"}`}
                title={entity.confirmed ? t("entityDetail.markUnconfirmed") : t("entityDetail.confirmEntity")}
              >
                {entity.confirmed ? t("entityDetail.confirmed") : t("entityDetail.confirmEntity")}
              </button>
              {confirmDelete ? (
                <>
                  <span className="entity-delete-question">{t("entityDetail.deleteQuestion")}</span>
                  <button
                    type="button"
                    disabled={deleteEntityMutation.isPending}
                    onClick={() => deleteEntityMutation.mutate()}
                    className="entity-delete-confirm"
                  >
                    {t("entityDetail.delete")}
                  </button>
                  <button
                    type="button"
                    onClick={() => setConfirmDelete(false)}
                    className="memory-detail-text-button"
                  >
                    {t("entityDetail.cancel")}
                  </button>
                </>
              ) : (
                <button
                  type="button"
                  onClick={() => setConfirmDelete(true)}
                  className="memory-detail-icon-button memory-detail-delete"
                  aria-label={t("entityDetail.deleteEntity")}
                  title={t("entityDetail.deleteEntity")}
                >
                  <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14" />
                  </svg>
                </button>
              )}
            </div>
          </div>
        </div>
      </header>

      <div className="memory-detail-grid">
        <section className="memory-detail-reading space-y-4" aria-label={t("entityDetail.readingLabel")}>
          {/* About: agent-recorded notes, curated here */}
          <div className="memory-detail-card">
            <div className="memory-detail-card-header">
              <h3 className="memory-detail-section-title">{t("entityDetail.aboutTitle")}</h3>
              {!showAddForm && (
                <button
                  type="button"
                  className="memory-detail-text-button"
                  onClick={() => setShowAddForm(true)}
                >
                  {t("entityDetail.addNote")}
                </button>
              )}
            </div>
            <div className="memory-detail-card-body">
              {(updateMutation.isError ||
                deleteMutation.isError ||
                addMutation.isError ||
                confirmObsMutation.isError) && (
                <p className="entity-error" role="alert">{t("entityDetail.saveError")}</p>
              )}
              {observations.length === 0 && !showAddForm && (
                <p className="entity-empty">{t("entityDetail.emptyAbout")}</p>
              )}
              <div className="entity-obs-list">
                {observations.map((obs) => (
                  <div key={obs.id} className="entity-obs-row">
                    <button
                      type="button"
                      className="memory-detail-state-button entity-obs-state"
                      onClick={() => confirmObsMutation.mutate({ id: obs.id, confirmed: !obs.confirmed })}
                      aria-label={obs.confirmed ? t("entityDetail.unconfirmObservation") : t("entityDetail.confirmObservation")}
                      title={obs.confirmed ? t("entityDetail.unconfirmObservation") : t("entityDetail.confirmObservation")}
                    >
                      <span className={`memory-detail-state-dot ${obs.confirmed ? "is-on" : ""}`} />
                    </button>
                    {editingObs === obs.id ? (
                      <input
                        value={editContent}
                        onChange={(e) => setEditContent(e.target.value)}
                        onBlur={() => {
                          if (cancelEditRef.current) {
                            cancelEditRef.current = false;
                            return;
                          }
                          saveEdit(obs.id);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") saveEdit(obs.id);
                          if (e.key === "Escape") {
                            // Stop it here: Main's window listener treats Escape as "back"
                            e.stopPropagation();
                            cancelEditRef.current = true;
                            setEditingObs(null);
                          }
                        }}
                        autoFocus
                        disabled={updateMutation.isPending}
                        className="entity-obs-input"
                        aria-label={t("entityDetail.editNote")}
                      />
                    ) : (
                      <button
                        type="button"
                        className={`entity-obs-content ${obs.confirmed ? "" : "is-unconfirmed"}`}
                        onClick={() => startEdit(obs)}
                        title={obs.source_agent ? `${t("entityDetail.editNote")} · ${obs.source_agent}` : t("entityDetail.editNote")}
                      >
                        {obs.content}
                      </button>
                    )}
                    {obs.confidence != null && (
                      <span className="entity-obs-confidence">{obs.confidence.toFixed(1)}</span>
                    )}
                    <button
                      type="button"
                      onClick={() => deleteMutation.mutate(obs.id)}
                      className="entity-obs-delete"
                      aria-label={t("entityDetail.deleteNote")}
                      title={t("entityDetail.deleteNote")}
                    >
                      <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <path d="M18 6L6 18M6 6l12 12" />
                      </svg>
                    </button>
                  </div>
                ))}
                {showAddForm && (
                  <div className="entity-obs-add">
                    <input
                      value={newObs}
                      onChange={(e) => setNewObs(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && newObs.trim() && !addMutation.isPending)
                          addMutation.mutate(newObs.trim());
                        if (e.key === "Escape") {
                          // Stop it here: Main's window listener treats Escape as "back"
                          e.stopPropagation();
                          setShowAddForm(false);
                          setNewObs("");
                        }
                      }}
                      placeholder={t("entityDetail.notePlaceholder", { name: entity.name })}
                      autoFocus
                      disabled={addMutation.isPending}
                      className="entity-obs-input"
                      aria-label={t("entityDetail.addNote")}
                    />
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Connections: one-hop map + relation ledger */}
          <div className="memory-detail-card">
            <div className="memory-detail-card-header">
              <h3 className="memory-detail-section-title">{t("entityDetail.connectionsTitle")}</h3>
              {relations.length > 0 && <span className="entity-count">{relations.length}</span>}
            </div>
            <div className="memory-detail-card-body">
              {relations.length === 0 ? (
                <p className="entity-empty">{t("entityDetail.emptyConnections")}</p>
              ) : (
                <>
                  <EntityGraph
                    name={entity.name}
                    neighbors={shownNeighbors}
                    hiddenCount={hiddenCount}
                    onEntityClick={onEntityClick}
                  />
                  <div className="entity-relation-list">{sortedRelations.map(relationRow)}</div>
                </>
              )}
            </div>
          </div>
        </section>

        <aside className="memory-detail-context" aria-label={t("entityDetail.contextLabel")}>
          <section className="memory-detail-rail-panel memory-detail-metadata-panel">
            <RailPanelTitle>{t("entityDetail.detailsTitle")}</RailPanelTitle>
            <div className="memory-detail-metadata-list">
              <MetadataRow label={t("entityDetail.typeLabel")}>
                <span className="entity-meta-mono">{entity.entity_type}</span>
              </MetadataRow>
              {space && (
                <MetadataRow label={t("entityDetail.spaceLabel")}>
                  <span className="entity-meta-mono">{space}</span>
                </MetadataRow>
              )}
              <MetadataRow label={t("entityDetail.statusLabel")}>
                <span className={`memory-detail-chip ${entity.confirmed ? "success" : "warning"}`}>
                  {entity.confirmed ? t("entityDetail.confirmed") : t("entityDetail.unconfirmed")}
                </span>
              </MetadataRow>
              {entity.confidence != null && (
                <MetadataRow label={t("entityDetail.confidenceLabel")}>
                  <span className="entity-meta-mono">{entity.confidence.toFixed(2)}</span>
                </MetadataRow>
              )}
              {entity.source_agent && (
                <MetadataRow label={t("entityDetail.sourceLabel")}>
                  <span className="entity-meta-mono">{entity.source_agent}</span>
                </MetadataRow>
              )}
              <MetadataRow label={t("entityDetail.firstSeen")}>
                <span className="entity-meta-mono" title={absoluteDate(entity.created_at)}>
                  {new Date(entity.created_at * 1000).toLocaleDateString(locale)}
                </span>
              </MetadataRow>
              <MetadataRow label={t("entityDetail.updatedLabel")}>
                <span className="entity-meta-mono" title={absoluteDate(entity.updated_at)}>
                  {timeAgo(entity.updated_at, locale)}
                </span>
              </MetadataRow>
            </div>
          </section>
        </aside>

        {linkedMemories.length > 0 && (
          <div className="memory-detail-context-dock">
            <section className="memory-detail-rail-panel memory-detail-secondary-panel">
              <div className="memory-detail-panel-heading">
                <h3 className="memory-detail-rail-title">{t("entityDetail.appearsTitle")}</h3>
                <span className="entity-appears-hint">{t("entityDetail.appearsHint")}</span>
              </div>
              <div className="memory-detail-related-grid">
                {linkedMemories.map((mem) => {
                  const facet = mem.memory_type ?? null;
                  const facetClass = facet ? FACET_COLORS[facet] : null;
                  return (
                    <button
                      key={mem.id}
                      type="button"
                      className="memory-detail-related-card"
                      onClick={() => onMemoryClick?.(mem.source_id)}
                    >
                      <span className="memory-detail-context-row-body">
                        <span className="memory-detail-related-copy line-clamp-2">{mem.content}</span>
                        <span className="memory-detail-related-meta">
                          {facet && facetClass && (
                            <span className={`memory-detail-related-facet border ${facetClass}`}>{facet}</span>
                          )}
                          {mem.is_archived && <span>{t("entityDetail.archived")}</span>}
                        </span>
                      </span>
                      <svg
                        className="memory-detail-related-chevron"
                        aria-hidden="true"
                        width="12"
                        height="12"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2"
                      >
                        <path d="M9 18l6-6-6-6" />
                      </svg>
                    </button>
                  );
                })}
              </div>
            </section>
          </div>
        )}
      </div>
    </main>
  );
}
