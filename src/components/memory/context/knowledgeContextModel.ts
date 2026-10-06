// SPDX-License-Identifier: AGPL-3.0-only
import type { GraphRef, KnowledgeGraph } from "../../../lib/tauri";

export type ContextGroup = "usedIn" | "sources" | "topics" | "linkedPages" | "linkedFrom";
export interface ContextLink extends GraphRef { title: string; group: ContextGroup }
const key = (ref: GraphRef) => `${ref.kind}:${ref.id}`;

/** Only stored, typed one-hop relationships; never infer usage from similarity. */
export function knowledgeContextLinks(graph: KnowledgeGraph, current: GraphRef): ContextLink[] {
  const nodes = new Map<string, string>();
  for (const page of graph.pages) nodes.set(key({kind:"page", id:page.id}), page.title);
  for (const memory of graph.memories) nodes.set(key({kind:"memory", id:memory.source_id}), memory.title);
  for (const entity of graph.entities) nodes.set(key({kind:"entity", id:entity.id}), entity.name);
  const links = new Map<string, ContextLink>();
  const add = (ref: GraphRef, group: ContextGroup) => {
    if (key(ref) === key(current) || !nodes.has(key(ref))) return;
    links.set(`${group}:${key(ref)}`, {...ref, title:nodes.get(key(ref)) ?? "", group});
  };
  for (const link of graph.page_links) {
    if (key(link.from) === key(current)) {
      if (link.link_type === "cites" && link.to.kind === "memory") add(link.to,"sources");
      else if (link.link_type === "about" && link.to.kind === "entity") add(link.to,"topics");
      else if (link.link_type === "wikilink" && link.to.kind === "page") add(link.to,"linkedPages");
    }
    if (key(link.to) === key(current)) {
      if (link.link_type === "cites" && link.from.kind === "page" && current.kind === "memory") add(link.from,"usedIn");
      else if (link.link_type === "wikilink" && link.from.kind === "page") add(link.from,"linkedFrom");
    }
  }
  if (current.kind === "memory") {
    for (const link of graph.memory_links) {
      if (link.memory_id === current.id) add({kind:"entity",id:link.entity_id},"topics");
    }
  }
  return [...links.values()];
}
