// SPDX-License-Identifier: AGPL-3.0-only
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { FileText, Circle, Hash, ArrowSquareOut } from "@phosphor-icons/react";
import { getKnowledgeGraph, type GraphRef } from "../../../lib/tauri";
import { knowledgeContextLinks, type ContextLink, type ContextGroup } from "./knowledgeContextModel";
import "./knowledge-context.css";

type Props = { standaloneGraph?: boolean; diagramOnly?: boolean; kind: "page" | "memory"; id: string; title: string;
  onOpenGraph?: () => void;
  onNavigatePage?: (id: string) => void; onNavigateMemory?: (id: string) => void; onNavigateEntity?: (id: string) => void };
const GROUPS: ContextGroup[] = ["usedIn", "sources", "topics", "linkedPages", "linkedFrom"];
function KindIcon({kind}: {kind: GraphRef["kind"]}) {
  const Icon = kind === "page" ? FileText : kind === "memory" ? Circle : Hash;
  return <Icon aria-hidden="true" size={14} />;
}

export default function KnowledgeContext({kind,id,title,onOpenGraph,onNavigatePage,onNavigateMemory,onNavigateEntity,standaloneGraph=false,diagramOnly=false}: Props) {
  const {t} = useTranslation();
  const graph = useQuery({queryKey:["knowledge-graph"],queryFn:getKnowledgeGraph,staleTime:0,refetchInterval:120_000,retry:false});
  const links = useMemo(() => graph.data ? knowledgeContextLinks(graph.data,{kind,id}) : [],[graph.data,kind,id]);
  const [expanded, setExpanded] = useState(false);
  const open = (node: GraphRef) => {
    if (node.kind === "page") onNavigatePage?.(node.id);
    else if (node.kind === "memory") onNavigateMemory?.(node.id);
    else onNavigateEntity?.(node.id);
  };
  const canOpen = (node: GraphRef) => node.kind === "page" ? !!onNavigatePage : node.kind === "memory" ? !!onNavigateMemory : !!onNavigateEntity;
  const label = (node: ContextLink) => node.title || t("knowledgeContext.untitled");
  const neighbours = [...new Map(links.map(node => [`${node.kind}:${node.id}`,node])).values()].slice(0,6);
  if (graph.isPending) return <p className="knowledge-context-state" role="status">{t("knowledgeContext.loadingLinks")}</p>;
  if (graph.isError) return <div className="knowledge-context-state" role="alert"><p>{t("knowledgeContext.linksError")}</p><button onClick={() => void graph.refetch()} type="button">{t("pageDetail.retry")}</button></div>;
  const graphAction = onOpenGraph && <button type="button" className="knowledge-context-open-graph" onClick={onOpenGraph}><ArrowSquareOut size={16} aria-hidden="true"/>{t("knowledgeContext.openInGraph")}</button>;
  if (!links.length) return <div className="knowledge-context"><p className="knowledge-context-state">{t("knowledgeContext.noLinks")}</p>{graphAction}</div>;
  const GraphContainer = standaloneGraph ? "section" : "details";
  return <div className="knowledge-context">
    <GraphContainer className="knowledge-local-map" {...(!standaloneGraph ? {open:true} : {})}>
      {!standaloneGraph && <summary>{t("knowledgeContext.localGraph")}</summary>}
      <div className="knowledge-local-map-diagram" role="group" aria-label={t("knowledgeContext.localGraph")}>
        <svg viewBox="0 0 300 220" preserveAspectRatio="none" aria-hidden="true">
          {neighbours.map((node,index) => {
            const angle = (index / neighbours.length) * Math.PI * 2 - Math.PI / 2;
            return <line key={`${node.kind}:${node.id}`} x1="150" y1="110" x2={150+100*Math.cos(angle)} y2={110+76*Math.sin(angle)} />;
          })}
        </svg>
        <span className="knowledge-local-map-center" title={title}><KindIcon kind={kind}/><span>{t(kind === "page" ? "knowledgeContext.noteKind" : "knowledgeContext.memoryKind")}</span></span>
        {neighbours.map((node,index) => {
          const angle = (index / neighbours.length) * Math.PI * 2 - Math.PI / 2;
          return <button type="button" key={`${node.kind}:${node.id}`} className={`knowledge-local-map-node knowledge-node-${node.kind}`}
            style={{left:`${50+(100/3)*Math.cos(angle)}%`,top:`${50+(76/2.2)*Math.sin(angle)}%`}}
            aria-label={t("knowledgeContext.openRelated",{kind:t(`knowledgeContext.kind.${node.kind}`),title:label(node)})} title={label(node)} disabled={!canOpen(node)} onClick={() => open(node)}>
            <KindIcon kind={node.kind}/><span>{label(node)}</span>
          </button>;
        })}
      </div>
      <p className="knowledge-context-hint">{t("knowledgeContext.graphHint")}</p>
      {graphAction}
    </GraphContainer>
    {!diagramOnly && GROUPS.map(group => {
      const matching = links.filter(node=>node.group===group); if (!matching.length) return null;
      return <section className="knowledge-context-section" key={group} aria-label={t(`knowledgeContext.${group}`)}>
        <h3>{t(`knowledgeContext.${group}`)}</h3>
        <ul>{(expanded ? matching : matching.slice(0,5)).map(node => <li key={`${node.kind}:${node.id}`}>
          <button type="button" disabled={!canOpen(node)} onClick={() => open(node)}><KindIcon kind={node.kind}/><span>{label(node)}</span></button>
        </li>)}</ul>
        {!expanded && matching.length>5 && <button type="button" className="knowledge-context-show-all" onClick={()=>setExpanded(true)}>{t("knowledgeContext.showAll")}</button>}
      </section>;
    })}
  </div>;
}
