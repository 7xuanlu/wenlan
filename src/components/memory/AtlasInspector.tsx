// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, ArrowRight, X } from "@phosphor-icons/react";
import type { GraphEdge, GraphNode, GraphNodeKind } from "../../lib/graph/model";
import { entityTypeLabel } from "../../lib/graph/typeFilter";
import "./atlasInspectorGroups.css";

const GROUP_ORDER: GraphNodeKind[] = ["page", "entity", "memory"];
const INITIAL_GROUP_LIMIT = 12;
const GROUP_PAGE_SIZE = 12;

type RelationDirection = "outgoing" | "incoming";

interface RelationSummary {
  type: string;
  direction: RelationDirection;
}

interface GroupedNeighbor {
  node: GraphNode;
  relations: RelationSummary[];
}

function relationsFor(nodeId: string, neighborId: string, edges: GraphEdge[]): RelationSummary[] {
  const unique = new Map<string, RelationSummary>();
  for (const edge of edges) {
    const isOutgoing = edge.source === nodeId && edge.target === neighborId;
    const isIncoming = edge.target === nodeId && edge.source === neighborId;
    if (!isOutgoing && !isIncoming) continue;
    const direction: RelationDirection = isOutgoing ? "outgoing" : "incoming";
    const key = `${direction}:${edge.type}`;
    if (!unique.has(key)) unique.set(key, { type: edge.type, direction });
  }
  return [...unique.values()];
}

function relationMatches(relations: RelationSummary[], filter: string): boolean {
  const normalized = filter.trim().toLocaleLowerCase();
  return !normalized || relations.some(({ type }) => type.toLocaleLowerCase().includes(normalized));
}

function groupHeadingId(group: GraphNodeKind): string {
  return `atlas-inspector-group-${group}`;
}

export interface AtlasInspectorProps {
  node: GraphNode;
  neighbors: GraphNode[];
  edges?: GraphEdge[];
  onSelect: (id: string) => void;
  onClose: () => void;
  onOpen?: () => void;
}

export default function AtlasInspector({
  node,
  neighbors,
  edges = [],
  onSelect,
  onClose,
  onOpen,
}: AtlasInspectorProps) {
  const { t } = useTranslation();
  const heading = useRef<HTMLHeadingElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const sectionRefs = useRef<Partial<Record<GraphNodeKind, HTMLElement | null>>>({});
  const [filter, setFilter] = useState("");
  const [expandedCounts, setExpandedCounts] = useState<Record<GraphNodeKind, number>>({
    page: INITIAL_GROUP_LIMIT,
    entity: INITIAL_GROUP_LIMIT,
    memory: INITIAL_GROUP_LIMIT,
  });

  useEffect(() => { heading.current?.focus({ preventScroll: true }); }, []);

  const grouped = useMemo(() => {
    const byKind = new Map<GraphNodeKind, GroupedNeighbor[]>();
    for (const kind of GROUP_ORDER) byKind.set(kind, []);
    for (const neighbor of neighbors) {
      const relations = relationsFor(node.id, neighbor.id, edges);
      if (!neighbor.name.toLocaleLowerCase().includes(filter.toLocaleLowerCase()) && !relationMatches(relations, filter)) continue;
      byKind.get(neighbor.kind)?.push({ node: neighbor, relations });
    }
    return byKind;
  }, [edges, filter, neighbors, node.id]);

  const totalMatches = [...grouped.values()].reduce((sum, group) => sum + group.length, 0);

  const changeFilter = (next: string) => {
    setFilter(next);
    setExpandedCounts({ page: INITIAL_GROUP_LIMIT, entity: INITIAL_GROUP_LIMIT, memory: INITIAL_GROUP_LIMIT });
  };

  const scrollToGroup = (kind: GraphNodeKind) => {
    const list = listRef.current;
    const section = sectionRefs.current[kind];
    if (!list || !section) return;
    const listRect = list.getBoundingClientRect();
    const sectionRect = section.getBoundingClientRect();
    list.scrollTo({ top: list.scrollTop + sectionRect.top - listRect.top, behavior: "auto" });
  };

  const renderRelation = (relation: RelationSummary, index: number) => (
    <span className="atlas-connection-relation" key={`${relation.direction}:${relation.type}:${index}`}>
      {relation.direction === "outgoing" ? <ArrowRight size={12} aria-hidden="true" /> : <ArrowLeft size={12} aria-hidden="true" />}
      <span>{relation.type.replaceAll("_", " ")}</span>
    </span>
  );

  return <aside className="atlas-inspector" aria-label={t("atlas.inspectorLabel")}>
    <div className="atlas-inspector-heading">
      <h2 ref={heading} tabIndex={-1}>{node.name}</h2>
      <button type="button" className="atlas-icon-button" onClick={onClose} aria-label={t("atlas.returnToMap")}><X size={16} /></button>
    </div>
    <p className="atlas-inspector-kind">
      {node.kind === "entity" ? t(`atlas.entityType.${node.entityType}`, { defaultValue: entityTypeLabel(node.entityType) }) : t(`atlas.layer.${node.kind}`)}
    </p>
    <button type="button" className="atlas-action" onClick={onClose}><ArrowLeft size={15} />{t("atlas.returnToMap")}</button>
    <h3 className="atlas-inspector-connections-heading">{t("atlas.connections")} <span>{neighbors.length}</span></h3>
    {neighbors.length > 12 && <input className="atlas-connection-filter" aria-label={t("atlas.filterConnections")} placeholder={t("atlas.filterConnections")} value={filter} onChange={(event) => changeFilter(event.target.value)} />}
    {totalMatches > 0 && <nav className="atlas-connection-summary" aria-label={t("atlas.connections")}>
      {GROUP_ORDER.map((kind) => {
        const count = grouped.get(kind)?.length ?? 0;
        if (count === 0) return null;
        return <button type="button" key={kind} aria-label={`${t(`atlas.layer.${kind}`)} ${count}`} onClick={() => scrollToGroup(kind)}>
          <span>{t(`atlas.layer.${kind}`)}</span><span>{count}</span>
        </button>;
      })}
    </nav>}
    <div ref={listRef} className="atlas-connection-list" aria-label={t("atlas.connections")}>
      {GROUP_ORDER.map((kind) => {
        const group = grouped.get(kind) ?? [];
        if (group.length === 0) return null;
        const headingId = groupHeadingId(kind);
        const visible = group.slice(0, expandedCounts[kind]);
        return <section key={kind} ref={(element) => { sectionRefs.current[kind] = element; }} className="atlas-connection-group" aria-labelledby={headingId}>
          <h4 id={headingId}>{t(`atlas.layer.${kind}`)} <span>{group.length}</span></h4>
          <div className="atlas-connection-group-rows">
            {visible.map(({ node: neighbor, relations }) => <button type="button" key={neighbor.id} aria-label={neighbor.name} onClick={() => onSelect(neighbor.id)}>
              <span className="atlas-connection-copy">
                <span className="atlas-connection-name">{neighbor.name}</span>
                {relations.length > 0 && <span className="atlas-connection-relations">{relations.slice(0, 2).map(renderRelation)}</span>}
              </span>
              <ArrowRight className="atlas-connection-navigate" size={14} aria-hidden="true" />
            </button>)}
          </div>
          {group.length > visible.length && <button type="button" className="atlas-more-connections" onClick={() => setExpandedCounts((counts) => ({ ...counts, [kind]: counts[kind] + GROUP_PAGE_SIZE }))}>{t("atlas.moreConnections")}</button>}
        </section>;
      })}
      {totalMatches === 0 && <p className="atlas-connection-empty">{t("atlas.noMatches")}</p>}
    </div>
    {onOpen && <button type="button" className="atlas-action atlas-open-detail" onClick={onOpen}>{t("atlas.openDetails")}<ArrowRight size={15} /></button>}
  </aside>;
}
