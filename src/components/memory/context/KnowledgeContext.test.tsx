import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import KnowledgeContext from "./KnowledgeContext";
import { getKnowledgeGraph } from "../../../lib/tauri";
vi.mock("../../../lib/tauri",()=>({getKnowledgeGraph:vi.fn()}));
function mount() {const open=vi.fn(); render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false}}})}><KnowledgeContext kind="memory" id="m" title="Saved fact" onNavigatePage={open}/></QueryClientProvider>);return open;}
describe("KnowledgeContext",()=>{
  it("opens a source-using page through both keyboard list and graph",async()=>{
    vi.mocked(getKnowledgeGraph).mockResolvedValue({entities:[],relations:[],memory_links:[],memories:[],pages:[{id:"p",title:"Project note",space:null,creation_kind:"source",entity_id:null,last_modified:""}],page_links:[{from:{kind:"page",id:"p"},to:{kind:"memory",id:"m"},link_type:"cites"}]});
    const open=mount();
    const section=await screen.findByRole("region",{name:"Used as a source in"});
    await userEvent.click(within(section).getByRole("button",{name:"Project note"}));
    await userEvent.click(screen.getByRole("button",{name:"Open note: Project note"}));
    expect(open).toHaveBeenCalledTimes(2);expect(open).toHaveBeenLastCalledWith("p");
  });
  it("reports failed graph reads instead of asserting no connections",async()=>{
    vi.mocked(getKnowledgeGraph).mockRejectedValue(new Error("offline"));mount();
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load connections");
    expect(screen.queryByText("No direct connections in the currently visible knowledge.")).toBeNull();
  });
});
