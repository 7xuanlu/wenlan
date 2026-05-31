# Six Contrarian Bets for Qi-Xuan Lu

You have spent ~6 weeks building Origin. 314 commits, ~37% on eval/CI/SEO process. The tech is good. The traction is zero. You asked to be challenged, so here is the blunt frame before the menu:

You are not stuck on the product. You are stuck because you keep doing the thing you are best at (rigor, systems, process) and avoiding the thing you are worst at (shipping a wedge, distribution). Every bet below is designed to force you out of the comfortable loop. Most of them mean putting Origin-the-product down.

The market context matters and it is not friendly to the obvious path:

- Mem0 raised $24M (Seed + Series A, Basis Set / Peak XV / YC / GitHub Fund), has 51k+ GitHub stars, 13M+ pip downloads, 80k+ developers signed up, and was picked as the *exclusive* memory provider for the AWS Agent SDK. It scores 93.4% on LongMemEval. [VERIFIED https://techcrunch.com/2025/10/28/mem0-raises-24m-from-yc-peak-xv-and-basis-set-to-build-the-memory-layer-for-ai-apps/] [VERIFIED https://mem0.ai/blog/state-of-ai-agent-memory-2026]
- Mem0 ships OpenMemory: a local-first, MCP-compatible memory server that already works with Claude Desktop, Cursor, Windsurf, VS Code. That is your exact positioning, shipped, funded, and distributed. [VERIFIED https://mem0.ai/blog/state-of-ai-agent-memory-2026]
- The agent-memory category now has 21 frameworks, 20 vector stores, dozens of MCP memory servers. It is crowded. [VERIFIED https://mem0.ai/blog/state-of-ai-agent-memory-2026]
- Meanwhile "eval-driven development" became a named discipline in early 2026, with Anthropic, Red Hat, Braintrust, and LangChain all publishing on it. Quality is the #1 barrier to shipping agents (32% cite it). [VERIFIED https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents] [VERIFIED https://developers.redhat.com/articles/2026/03/23/eval-driven-development-build-evaluate-ai-agents] [VERIFIED https://www.braintrust.dev/articles/eval-driven-development]

Read those two facts together. You are competing head-on with a funded, distributed incumbent in memory, while sitting on top of exactly the skill (eval rigor, AGENTS.md discipline, a real harness) that the market just started paying for. That asymmetry is the spine of this menu.

---

## Bet 1 (category a: radical repositioning of the SAME tech)

**Reposition Origin from "AI memory tool" to "the audit log and provenance layer for AI agents" — sell trust and traceability, not recall.**

**Thesis (why 10x not 10%).** Everyone is building memory. Almost nobody is building the boring compliance-grade layer underneath: who wrote this fact, from what source, when, with what confidence, and can I diff it. Your repo already has provenance, git-versioned artifacts, and local-first storage. That is not a memory feature. That is an *audit* feature. The buyers for audit are different (security, compliance, platform teams), they have budget, and they cannot use a SaaS that ships their data to a vendor. Local-first stops being a nerd preference and becomes the entire value prop. Memory is a 10% improvement in a crowded field. "The agent's actions are provenance-tracked and replayable on-device" is a 10x reframe into an empty field.

**Why contrarian.** Consensus says memory = better recall = higher benchmark score. You would be explicitly *not* competing on LongMemEval. You would tell people the benchmark race is a trap (Mem0 already won it at 93.4%) and that the real unmet need is verifiability.

**What to kill.** Stop tuning retrieval. Stop running LongMemEval/LoCoMo for score-chasing. Stop the SEO content. Freeze the recall feature set.

**Smallest experiment (<1 week).** Rewrite the README and landing one-liner to "provenance and audit log for AI agents, fully local." Build one screen/CLI command: `origin trace <fact-id>` that shows the full provenance chain + a git diff of how a memory changed over time. Post it to r/LocalLLaMA, the MCP Discord, and 5 platform-eng people on X with the framing "your agent's memory should be auditable." **Success:** 3+ unsolicited "I need this for compliance/security reasons" responses, or 1 person asking to pay/pilot. **Kill:** all the feedback is "cool, but how is this different from Mem0."

**Honest risk.** Provenance might be a vitamin, not a painkiller, until regulation forces it. You could be 18 months early. It fails if the people who care about audit are all in enterprises with 9-month sales cycles you cannot run solo.

---

## Bet 2 (category b: productize the PROCESS)

**Stop selling memory. Sell your AGENTS.md + eval-harness + agent-discipline system as a product: "the operating system for running coding agents with rigor."**

**Thesis (why 10x not 10%).** Your repo is unusual not because of what it does but because of *how it was built*. The AGENTS.md hierarchy, the L1-L8 test responsibility split, the eval citation discipline, the worktree-cleanup playbook, the "challenge assumptions / verify before claiming done" rules. That is a complete methodology for keeping AI agents honest and productive. Thousands of devs are now drowning in agents that hallucinate "done," skip tests, and gold-plate. You already solved that for yourself, in writing. Eval-driven development is a freshly-named, fast-growing discipline with the biggest labs publishing on it and enterprises citing quality as the #1 blocker. [VERIFIED https://developers.redhat.com/articles/2026/03/23/eval-driven-development-build-evaluate-ai-agents] You are not early here, you are *on time* with battle-tested artifacts most people are still hand-wringing about.

**Why contrarian.** It means admitting the product is the byproduct and the *scaffolding* is the asset. Founders are trained to never do this. Most people see their AGENTS.md as overhead; you would see it as the deliverable.

**What to kill.** Origin-the-memory-tool goes on the shelf as a reference implementation, not the headline. Stop adding memory features entirely.

**Smallest experiment (<1 week).** Extract your AGENTS.md system + eval-discipline rules + the L1-L8 model into a standalone repo: `agent-rigor` (a template + a CLI that scaffolds AGENTS.md, pre-commit/pre-push hooks, and an eval-harness skeleton into any repo). Write one sharp post: "I spent 6 weeks building an AI memory tool. The most valuable thing I made was the system that kept the agent honest. Here it is." Ship the repo, post to HN / r/ExperiencedDevs / X. **Success:** 100+ GitHub stars in 72 hours OR 3 people asking "can you set this up for my team." **Kill:** under 20 stars and no inbound. (Compare: a methodology repo that resonates clears 100 stars fast; memory MCP servers routinely sit under 50.)

**Honest risk.** Methodology repos are a dime a dozen and easy to dismiss as "just a markdown file." The moat is your credibility, which is thin without an audience yet. This bet pairs naturally with Bet 3.

---

## Bet 3 (category c: become the researcher/writer, not the founder)

**Publish your memory-eval findings as the canonical, brutally-honest field guide to AI memory benchmarks. Build the audience first; let the product follow the audience.**

**Thesis (why 10x not 10%).** You have something almost nobody in this space has: real, rigorous, multi-run eval data on LoCoMo and LongMemEval with citation discipline, env-stamping, and a documented refusal to compare across schema versions. The whole category is full of vendors quoting single cherry-picked numbers (Mem0's own "+26% accuracy" headlines). [VERIFIED https://weavai.app/blog/en/2026/05/09/mem0-review-2026-ai-agent-memory-king-26-accuracy/] An independent, no-product-to-sell, methodology-obsessed teardown of "what these memory benchmarks actually measure and where they lie" would travel. Audience is distribution, and distribution is the exact thing you lack. A great essay can do in one week what 6 weeks of commits did not: get you known. 10x is not a better tool, it is 5,000 people who now know your name and trust your judgment on memory and evals.

**Why contrarian.** Founders are told to build, not write. You would deliberately stop shipping code and spend a week shipping prose and charts. It also means publishing findings that may undercut your own product's reason to exist (e.g. "honestly, embedding-only retrieval gets you 90% of the way").

**What to kill.** Pause all feature work and SEO. The eval runs stop being internal QA and become the raw material for public writing.

**Smallest experiment (<1 week).** Write and publish ONE piece: "I ran LongMemEval and LoCoMo properly for 6 weeks. Here is what the AI-memory benchmark numbers don't tell you." Include per-case breakdowns, the adversarial-cat-5 contamination trap, and a methodology section. Post to HN, Lobsters, r/LocalLLaMA, X. **Success:** front page of HN OR 10k+ views OR 200+ new followers in a week. **Kill:** under 1k views and no comments worth replying to.

**Honest risk.** Writing well for an audience is a different muscle than writing rigorous internal docs; "correct but boring" gets ignored. It can also tip into all-talk-no-product. Mitigation: timebox to one piece, judge the signal, do not become a full-time content person unless the numbers say so.

---

## Bet 4 (category d: an absurdly specific wedge)

**Narrow Origin all the way down to one wedge: persistent, provenance-tracked memory for a single coding agent (Claude Code) inside one workflow — "never re-explain your codebase to your agent twice."**

**Thesis (why 10x not 10%).** "AI memory layer for everything" competes with Mem0 and loses. "The thing that remembers your architecture decisions, your gotchas, and your AGENTS.md conventions across Claude Code sessions, locally, with git-versioned provenance" is a knife. You already ship a Claude Code plugin and an MCP server. The pain is real and specific: agents forget the project's conventions every session and re-derive (often wrongly) what you already told them. Your own CLAUDE.md/AGENTS.md system is proof you feel this pain. A wedge this narrow can own a niche entirely, and niche-owned beats category-also-ran. 10x is being THE answer for one job, not the 15th answer for all jobs.

**Why contrarian.** It throws away 90% of the product surface and the general "memory layer" ambition. Solo devs hate narrowing because it feels like shrinking. It is actually how you win when an incumbent owns the broad category.

**What to kill.** Drop the general ingest pipelines, webpage ingest, the broad "personal agent memory" framing, the desktop-app ambitions. One agent, one workflow, one painkiller.

**Smallest experiment (<1 week).** Ship a dead-simple Claude Code plugin: it captures architectural decisions + gotchas during a session and auto-injects them into the next session's context, fully local, with a one-line provenance per fact. Use it yourself for a week on this very repo. Then give it to 5 Claude Code power users. **Success:** 3 of 5 say "I would keep this on" after a week, with a concrete moment it saved them. **Kill:** even you turn it off because it adds noise instead of signal.

**Honest risk.** Anthropic ships native memory/skills for Claude Code and flattens you overnight; platform-dependency is the classic wedge killer. Also the wedge may be too small to matter even if it works. Mitigation: pick the wedge precisely because *you* are the customer and can dogfood instantly.

---

## Bet 5 (category e: go where Rust + local-first is rare and valuable)

**Stop competing in the Python-saturated agent-tooling space. Take your Rust + libSQL + local-first systems skill to a domain where on-device, no-cloud, low-latency is a hard requirement and Python can't follow you.**

**Thesis (why 10x not 10%).** The agent-memory field is a Python monoculture (Mem0 = 13M pip downloads). Your Rust systems ability is a *commodity* there and an *edge* somewhere else. Local-first + Rust is genuinely scarce and genuinely valued in: privacy-regulated on-device inference (health, legal, finance notes), edge/embedded agents, desktop apps that refuse the cloud, or dev-tools that need to be a single fast binary. In those rooms, "I can build a correct, fast, fully-local Rust daemon with vector search and a real eval harness" is rare and hireable/contractable at a premium. [INFERRED] You are trying to win a popularity contest in the one ecosystem where your main weapon doesn't count.

**Why contrarian.** It means possibly abandoning the AI-memory category entirely and following the skill, not the trend. It may mean contracting/consulting income over startup-equity dreams. Founder culture treats that as giving up.

**What to kill.** The whole "I am building an AI memory startup" identity, if the experiment says so. Pivot the artifact from product to portfolio/proof-of-skill.

**Smallest experiment (<1 week).** Write 2-3 sharp outreach notes positioning yourself as "Rust + local-first + on-device AI, with eval rigor" and send to: 5 local-first/edge-AI companies hiring or contracting, 1-2 relevant Show HN or "who's hiring" threads, and the Ink & Switch / local-first community. Lead with Origin as the proof artifact. **Success:** 2+ real conversations (call booked, contract scoped, or serious inbound) in a week. **Kill:** zero responses worth a reply.

**Honest risk.** This is closer to a job search than a moonshot, and it may feel like surrender. But "10x" can mean 10x your leverage and income, not just a startup outcome. It fails if you treat it half-heartedly while secretly still polishing Origin.

---

## Bet 6 (category f: wildcard)

**Run Origin as a public, no-product, in-the-open research lab: livestream/build-in-public the eval work itself, and let the *process* become the audience magnet and the brand.**

**Thesis (why 10x not 10%).** Your single biggest untapped asset is that your *process is genuinely impressive* and almost nobody gets to see it. The L1-L8 testing model, the citation discipline, the worktree hygiene, the "verify before claiming done" enforcement. Build-in-public usually fails because the building is boring. Yours is not. Turn the eval runs, the baseline comparisons, the "here's a regression I caught and how" into a public series (X threads, a weekly log, maybe a livestream of an eval run + teardown). The rigor that gives you *zero* distribution today becomes the *content* that earns it. This is a wildcard because it bets that your weakness (over-investing in process) is actually a disguised strength once it is visible.

**Why contrarian.** It says the answer to "I have no distribution" is not "do marketing" but "make the work itself the marketing." Most rigor-heavy devs hide their process as embarrassing overhead. You would weaponize it.

**What to kill.** Private polishing. If you do the work, you do it in public, or it doesn't count this month.

**Smallest experiment (<1 week).** Pick your most interesting eval finding or a live regression hunt. Document it in real time as a 5-tweet thread + one short Loom/asciinema of the harness running. Frame: "watch me catch an AI-memory regression with a proper eval harness." **Success:** one thread clears 10k impressions or 50+ meaningful engagements. **Kill:** consistently under 500 impressions across two attempts.

**Honest risk.** Build-in-public is a graveyard; most accounts shout into the void for months. It can also become a procrastination vehicle that *feels* like progress. It only works if the underlying work is genuinely interesting, which is the one thing I actually believe about you.

---

## The Anti-Bet: the path you are most likely on

**Most likely path: you keep polishing Origin. Another eval variant. Another retrieval channel. A bit more SEO. Cleaner CI. A new benchmark baseline.**

**Blunt 6-month projection.** You will have ~600 commits, an even more beautiful repo, a higher LongMemEval score, and roughly the same traction you have now: near zero. Here is the evidence-style reasoning:

1. The thing blocking you is distribution and a wedge, and none of the polishing work touches either. Adding rigor to an undistributed product compounds rigor, not reach. [OPINION, but well-supported by your own commit breakdown: 37% on eval/CI/SEO with little traction means the marginal return on more process is near zero.]
2. The category leader is funded, distributed, and ahead on the exact metric you keep tuning. Mem0 is at 93.4% LongMemEval with $24M and AWS distribution. [VERIFIED https://mem0.ai/blog/state-of-ai-agent-memory-2026] You cannot out-polish a funded incumbent at their own game as a solo dev. Six more months of polishing closes the gap from "far behind and unknown" to "slightly less far behind and still unknown."
3. Polishing is psychologically rewarding for someone with your strengths, which is exactly why it is the trap. It produces visible progress (green CI, higher scores) with zero exposure to the thing you fear (shipping a wedge, being seen, being told no). The comfort is the tell.

In 6 months on this path you will be more skilled, more frustrated, and no more known. The repo will be a monument to rigor that nobody asked for. That is the default. Every bet above exists to break it.

---

## Conviction Ranking (honest)

Which I actually believe, versus which are just options on the table:

1. **Bet 3 (become the researcher/writer).** Highest conviction. It directly attacks your real bottleneck (distribution) using your real strength (rigor), costs one week, and has almost no downside. Even if everything else fails, you come out known. The market timing on eval-driven-development is real and now. This is the one I would do first, this week. *Believe it.*

2. **Bet 2 (productize the process).** High conviction, and it stacks on Bet 3. Your AGENTS.md/eval system is genuinely differentiated and the discipline it encodes is in demand. Lower certainty than Bet 3 only because methodology repos are easy to dismiss. *Believe it, especially paired with Bet 3.*

3. **Bet 1 (reposition to provenance/audit).** Medium-high conviction. It is the smartest move for the *existing tech* because it stops the unwinnable fight with Mem0 and runs to an emptier field. Risk: might be early. But the reframe alone is worth testing in a week. *Believe the reframe; unsure on timing.*

4. **Bet 5 (follow the Rust + local-first skill elsewhere).** Medium conviction as a *life* move, lower as a *moonshot*. It is probably the highest-expected-value path for your income and leverage, and the one your founder ego will resist hardest. Ranked here, not higher, because it is partly a retreat from the mission. *Believe it as a floor; you should run the experiment even if only as insurance.*

5. **Bet 4 (absurdly narrow wedge).** Medium-low conviction. The wedge is real and dogfoodable, but platform risk (Anthropic ships native memory) is severe and the ceiling may be low. Worth a week because you are the customer. *An option, not a conviction.*

6. **Bet 6 (public research lab / build-in-public).** Lowest conviction as a standalone. Build-in-public mostly fails and can become procrastination cosplaying as progress. I rank it last *as its own bet*, but it is a great *delivery mechanism* for Bets 2 and 3. *Use it as a tactic, not a strategy.*

The honest meta-move: do Bet 3 this week, fold Bet 2 into it, and let the result tell you whether to lean into repositioning (Bet 1) or follow the skill out (Bet 5). Stop polishing on day one.
