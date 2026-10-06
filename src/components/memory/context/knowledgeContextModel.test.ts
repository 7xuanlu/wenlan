import { describe, expect, it } from "vitest";
import { knowledgeContextLinks } from "./knowledgeContextModel";
import type { KnowledgeGraph } from "../../../lib/tauri";
const graph: KnowledgeGraph = {
  entities: [], relations: [], memory_links: [],
  memories:[{source_id:"m",title:"Saved fact",memory_type:"fact",space:null,confirmed:false,last_modified:0}],
  pages:[{id:"p",title:"A note",space:null,creation_kind:"source",entity_id:null,last_modified:""}],
  page_links:[{from:{kind:"page",id:"p"},to:{kind:"memory",id:"m"},link_type:"cites"}],
};
describe("knowledgeContextLinks",()=>{
  it("resolves persisted source links in both directions without needing confirmed status",()=>{
    expect(knowledgeContextLinks(graph,{kind:"memory",id:"m"})).toEqual([{kind:"page",id:"p",title:"A note",group:"usedIn"}]);
    expect(knowledgeContextLinks(graph,{kind:"page",id:"p"})).toEqual([{kind:"memory",id:"m",title:"Saved fact",group:"sources"}]);
  });
  it("does not infer usage from inventory proximity or an absent target",()=>{
    expect(knowledgeContextLinks({...graph,page_links:[]},{kind:"memory",id:"m"})).toEqual([]);
    expect(knowledgeContextLinks({...graph,pages:[]},{kind:"memory",id:"m"})).toEqual([]);
  });
  it("separates namespaces and deduplicates source edges",()=>{
    expect(knowledgeContextLinks({...graph,page_links:[...graph.page_links,...graph.page_links]},{kind:"memory",id:"m"})).toHaveLength(1);
    expect(knowledgeContextLinks(graph,{kind:"page",id:"m"})).toEqual([]);
  });
});
