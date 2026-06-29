# Wedge Validation: Provenance-Enforced, Git-Versioned, On-Device AI Memory for Claude Code

Date: 2026-05-31. Subject: Qi-Xuan Lu / Origin.
Wedge under test: "provenance-enforced, git-versioned, on-device AI memory for Claude Code developers."
ICP: privacy-conscious senior engineers / indie hackers who live in Claude Code and distrust cloud memory.

Mandate: find evidence the wedge is real, or evidence it is a values-driven niche too small to matter. No cheerleading.

---

## VERDICT: YELLOW (real pain, real market, but the wedge as worded is contested ground and the differentiators are weak)

The core pain (context loss between AI coding sessions) is loud, stated, and growing. The market (Claude Code) is large and expanding fast. But the specific wedge framing splits into a strong part and a weak part:

- STRONG: "persistent memory for Claude Code, on-device." Real demand, large TAM, but a crowded field of free open-source competitors already shipping it.
- WEAK: "provenance-enforced" as the headline differentiator. The demand for source-cited memory exists mostly as enterprise governance/compliance language, not as an indie-hacker want. It is closer to a builder's value than a stated user pull.

YELLOW means: pursue, but reposition the wedge around the loud pain (context loss + on-device control) and treat provenance as a quiet trust feature, not the headline. Do not bet the company on provenance being the thing people ask for, because they are not asking for it.

---

## The five strongest pieces of evidence FOR the wedge

1. The context-loss pain is real, stated, and emotionally charged. Cursor's official community forum has dedicated bug threads titled "Cursor Loses Context Mid-Session + Frequent 'Amnesia' Issues" and "AI Context Loss and Repetitive Documentation Review." Users describe "every new chat starts fresh, with decisions, architecture context, and debugging sessions all gone." [VERIFIED https://forum.cursor.com/t/cursor-loses-context-mid-session-frequent-amnesia-issues-in-recent-updates/85230] [VERIFIED https://forum.cursor.com/t/ai-context-loss-and-repetitive-documentation-review-after-1-2-4-update/122560]

2. The market is huge and growing fast. Claude Code crossed 4.2M weekly active developers in Q1 2026 and is reported as the most-used AI coding agent, with 71% of regular AI-agent users naming it primary. [VERIFIED https://www.gradually.ai/en/claude-code-statistics/] Even a small power-user slice is a meaningful TAM (see market math below).

3. People are actively building and starring memory solutions for exactly this problem. doobidoo/mcp-memory-service has ~1.9k stars and defaults to local SQLite-Vec; mem0 has 41k+ stars and served 186M API calls last quarter. Demand is validated by the existence and traction of an entire memory-MCP category. [VERIFIED https://github.com/doobidoo/mcp-memory-service] [VERIFIED https://github.com/mem0ai/mem0]

4. On-device / local-first AI has real, quantified momentum. Ollama reported 52M monthly downloads in Q1 2026, up from ~100K in Q1 2023. Local-first is a live developer-conference and Hacker News topic, not a fringe ideology. [VERIFIED https://github.com/ollama/ollama] [VERIFIED https://www.infralovers.com/blog/2025-08-13-ollama-2025-updates/]

5. The cloud-memory trust gap is a stated developer complaint, not just a privacy abstraction. "OpenAI's memory is opaque to developers, with limited ability to inspect, manage, or correct what's been stored." Memory poisoning and stale-fact problems are documented as the real risk in agent memory ("the biggest risk in AI agents isn't hallucination, it's stale memory served with high confidence"). This is the seed of the provenance value prop. [VERIFIED https://www.mindstudio.ai/blog/agent-memory-infrastructure-mem0-vs-openai] [VERIFIED https://dev.to/ac12644/your-ai-agent-is-confidently-lying-and-its-your-memory-systems-fault-4d82]

---

## The five strongest pieces of evidence AGAINST the wedge

1. The wedge is already substantially built, free, and open-source. yuvalsuede/memory-mcp markets itself as "Persistent memory for Claude Code, never lose context between sessions," is "100% local files, your git repo," and does automatic git snapshots with rollback. That is on-device + git-versioned + Claude Code, three of Origin's four claimed pillars, shipped today for free. Origin's only un-copied pillar is provenance. [VERIFIED https://github.com/yuvalsuede/memory-mcp]

2. The market accepts cloud memory when it is convenient. mem0 (cloud-leaning, 41k stars, 186M API calls) and OpenAI/Cursor built-in memory are the mainstream adoption path. The repeated framing is "convenience vs privacy" with most users choosing convenience: "users accustomed to seamless cloud synchronization struggle with the friction of local-first." On-device is the minority preference. [VERIFIED https://github.com/mem0ai/mem0] [VERIFIED https://rxdb.info/articles/local-first-future.html]

3. Provenance is a builder's value, not a stated indie-hacker want. Searches for individual developers asking for source-cited memory return enterprise governance, compliance, audit-trail, and regulator language, not indie-hacker feature requests. The demand is "logging memory operations for auditability," "regulators confirming deletion," "watsonx audit trails." That is a sales motion to enterprises, not a wedge into solo Claude Code users. [VERIFIED https://atlan.com/know/ai-agent-memory-governance/] [VERIFIED https://www.newamerica.org/insights/ai-agents-and-memory/] No evidence found of a solo dev saying "I won't use AI memory unless every fact is source-cited."

4. The category is crowded with at least five named competitors, several backed or well-starred. mcp-memory-keeper, mcp-memory-service (1.9k stars), memory-mcp, mem0 (41k stars), WhenMoon Memory. A solo dev entering a category this dense needs a differentiator users actively ask for, and the chosen differentiator (provenance) is the one with the weakest stated pull. [VERIFIED https://github.com/mkreyman/mcp-memory-keeper] [VERIFIED https://github.com/doobidoo/mcp-memory-service]

5. "Will install a daemon" is a real adoption tax that shrinks an already-niche ICP. The mainstream solutions are MCP servers spawned on demand (zero standing process). Origin requires a launchd/systemd daemon. The intersection of {Claude Code user} AND {wants on-device} AND {distrusts cloud} AND {will run a persistent background daemon} AND {cares about provenance} is the multiplication of several minority preferences. Each AND cuts the funnel. [INFERRED from competitor architecture: most memory-MCP tools are stdio MCP servers, not daemons; see mem0/memory-mcp setup docs] [VERIFIED https://github.com/yuvalsuede/memory-mcp]

---

## Question-by-question findings

### 1. Demand for LOCAL/on-device memory specifically, or do people accept cloud?

Both are true, and the split matters. Local-first has genuine, quantified momentum: Ollama 52M monthly downloads, up ~520x from Q1 2023 [VERIFIED https://github.com/ollama/ollama]. But the mainstream memory-layer adoption is cloud or cloud-hybrid (mem0, OpenAI memory). Local-first is described repeatedly as the principled-minority choice: "developers are betting that a growing segment of users will choose autonomy over ease" [VERIFIED https://tech.grahammiranda.com/why-local-first-software-is-making-a-comeback-and-what-it-means-for-privacy]. The on-device segment is real and large in absolute terms, but it is a segment, not the default. Verdict on Q1: local demand exists and is growing, but it is a deliberate minority, not a mass want.

### 2. Is "losing context between sessions" a felt, stated pain?

Yes. This is the strongest signal in the whole investigation. Cursor's own forum hosts multi-thread "amnesia" bug reports with users describing architecture context and debugging sessions "all gone" [VERIFIED https://forum.cursor.com/t/cursor-loses-context-mid-session-frequent-amnesia-issues-in-recent-updates/85230]. An entire ecosystem of memory-MCP servers exists solely to solve "never lose context between sessions" [VERIFIED https://github.com/yuvalsuede/memory-mcp]. The pain is loud, recurring, and product-shaped. Caveat: it is loud enough that everyone is already attacking it.

### 3. Provenance/trust: user want or builder value?

Mostly builder value, with an enterprise-governance shadow. The demand for source-cited / auditable memory shows up in compliance, governance, regulator, and memory-poisoning contexts [VERIFIED https://atlan.com/know/ai-agent-memory-governance/] [VERIFIED https://dev.to/mjmirza/agent-memory-poisoning-the-4-stage-enterprise-damage-chain-20fi]. The underlying problem (stale/poisoned/hallucinated memory served as fact) is real and worsening [VERIFIED https://dev.to/ac12644/your-ai-agent-is-confidently-lying-and-its-your-memory-systems-fault-4d82]. But individual Claude Code users are not posting "I need provenance." This is the wedge's weakest link: a real problem that users feel as "the AI told me something wrong" rather than as "I want source citations." Origin would have to do the work of converting a latent trust problem into a stated want. Possible, but it is education-first marketing, which is expensive for a solo dev.

### 4. Market-size reality check

[ESTIMATE: show math]
- Claude Code WAU: 4.2M [VERIFIED https://www.gradually.ai/en/claude-code-statistics/].
- Power users who customize their setup with MCP/memory tooling: assume 10% (MCP is mainstream but most users never install a server). 4.2M x 0.10 = 420K.
- Of those, want on-device / distrust cloud enough to prefer local: assume 25% (local-first is a deliberate minority; Ollama-scale interest suggests this is not tiny). 420K x 0.25 = 105K.
- Of those, will run a standing daemon (vs on-demand MCP): assume 50%. 105K x 0.50 = 52.5K.
- Of those, provenance is a purchase driver (not just nice-to-have): assume 30%. 52.5K x 0.30 = ~15.7K.

Reachable serious-prospect pool: roughly 15K to 100K depending on whether provenance is required or optional. At a $5-15/mo prosumer price and 2-5% conversion of the broad on-device pool (~105K), that is ~2K-5K paying users = ~$120K-$900K ARR ceiling for a solo dev. That is a viable indie-scale business, not a venture-scale one. The numbers support a lifestyle/indie product, not a fundable startup, on the provenance framing alone. Drop "provenance-required" and the reachable pool jumps to the 50K-100K range.

### 5. The kill case (argued honestly)

The strongest argument to repick: the wedge picks the loud pain (context loss) but differentiates on the quiet feature (provenance), and the loud pain is already served free by open-source tools that also do on-device and git versioning. yuvalsuede/memory-mcp alone covers three of four pillars at zero cost [VERIFIED https://github.com/yuvalsuede/memory-mcp]. A new entrant whose only unique pillar is the one users do not ask for is structurally disadvantaged: it must educate the market on why provenance matters, while free competitors win the users who just want their context back. Meanwhile the people who genuinely demand provenance (auditability, deletion proof) are enterprises with procurement, compliance, and security review, which is a brutal sales motion for one solo dev. The kill case is: the wedge is squeezed between free OSS on the convenience axis and enterprise-sales gravity on the provenance axis, with no clean lane for a solo prosumer product.

Why it is YELLOW not RED: the pain is real, the market is large and growing, on-device demand is genuine, and the competitors are mostly hobbyist OSS with no polish, no daemon-grade reliability, and no trust/provenance story at all. A solo dev who ships the best-engineered on-device memory daemon for Claude Code, with provenance as a quiet trust differentiator rather than the headline, can carve a defensible niche. The wedge is not wrong. The emphasis is wrong.

---

## Recommendation

1. Reposition the headline from "provenance-enforced" to "your Claude Code's memory, on your machine, that you can trust and roll back." Lead with the loud pain (never lose context, fully local, git-versioned) and let provenance be the reason-to-believe-the-trust-claim, not the pitch.
2. Beat the free OSS on engineering quality: daemon reliability, install UX, hybrid retrieval, the things hobbyist MCP servers do badly.
3. Treat provenance as the moat you build quietly and the thing you sell loudly only when an enterprise asks. Do not spend solo-dev marketing budget educating indie hackers on why citations matter.
4. Validate before scaling: post the on-device + rollback framing in r/ClaudeAI / r/LocalLLaMA and the Cursor/Claude forums where the amnesia complaints live, and measure whether "on-device + trust" or "provenance" gets the engagement. Let the market pick the headline.

---

## Source list

- https://forum.cursor.com/t/cursor-loses-context-mid-session-frequent-amnesia-issues-in-recent-updates/85230
- https://forum.cursor.com/t/ai-context-loss-and-repetitive-documentation-review-after-1-2-4-update/122560
- https://www.gradually.ai/en/claude-code-statistics/
- https://github.com/doobidoo/mcp-memory-service
- https://github.com/yuvalsuede/memory-mcp
- https://github.com/mkreyman/mcp-memory-keeper
- https://github.com/mem0ai/mem0
- https://github.com/ollama/ollama
- https://www.infralovers.com/blog/2025-08-13-ollama-2025-updates/
- https://www.mindstudio.ai/blog/agent-memory-infrastructure-mem0-vs-openai
- https://dev.to/ac12644/your-ai-agent-is-confidently-lying-and-its-your-memory-systems-fault-4d82
- https://dev.to/mjmirza/agent-memory-poisoning-the-4-stage-enterprise-damage-chain-20fi
- https://atlan.com/know/ai-agent-memory-governance/
- https://www.newamerica.org/insights/ai-agents-and-memory/
- https://tech.grahammiranda.com/why-local-first-software-is-making-a-comeback-and-what-it-means-for-privacy
- https://rxdb.info/articles/local-first-future.html
