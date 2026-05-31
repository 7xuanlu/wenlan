# Launch Playbook: Open-Source Local-First Memory / MCP Server for Claude Code

Audience: a solo dev with no prior successful launch and near-zero distribution. This is mechanics, not motivation. Every claim is tagged.

**Tag key:**
- `[VERIFIED url]` = documented in a primary source or a credible analysis with data.
- `[INFERRED]` = a reasonable conclusion drawn from verified facts, not stated directly.
- `[OPINION]` = my judgment / folk wisdom. Treat as a hypothesis, not a rule.

**Number honesty:** where a launch number could not be independently confirmed, it is marked "reported, unverified." No numbers were invented.

---

## 0. The one thing, if you read nothing else

Ship a thing that runs in one command and a 30-second GIF that proves it, then post a neutral `Show HN:` title and answer every comment in the first 3 hours. The single highest-leverage mechanic is **being present and responsive in your own thread the moment it goes live**, because early discussion is what HN's ranking and readers reward, and the window is short: the half-life of a Show HN is ~24 hours and ~92% of the GitHub-star impact is over within 48 hours. `[VERIFIED https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/]`

---

## 1. Show HN mechanics that work in 2025-2026

### 1a. The documented rules (these are real, follow them exactly)

From the official Show HN page and HN guidelines:

- **It must be something people can actually try.** "Show HN is for something you've made that other people can play with." A landing page, market test, fundraiser, blog post, or curated list does NOT qualify. `[VERIFIED https://news.ycombinator.com/showhn.html]`
- **Put the URL in the URL field, leave the text box blank.** "Posts without URLs get penalized." Your repo (GitHub) is a valid URL target. `[VERIFIED https://gist.github.com/tzmartin/88abb7ef63e41e27c2ec9a5ce5d9b5f9]`
- **Then add your own comment** with the backstory: how you came to build it and what's different. `[VERIFIED https://gist.github.com/tzmartin/88abb7ef63e41e27c2ec9a5ce5d9b5f9]`
- **Neutral title, no marketing.** No hype, no exclamation points, no site name in the title, no "revolutionary." Marketing-sounding titles get flagged on sight. `[VERIFIED https://news.ycombinator.com/newsguidelines.html]` `[VERIFIED https://syften.com/blog/hacker-news-marketing/]`
- **Make it easy to try with no signup.** "Preferably without having to sign up, get a confirmation email, and other such barriers." HN users get ornery at hoops. `[VERIFIED https://news.ycombinator.com/showhn.html]`
- **Do not use your company/project name as your HN username.** It reads as promotion, not participation. `[VERIFIED https://news.ycombinator.com/showhn.html]`

### 1b. What gets flagged or killed

- **Asking for upvotes** (anywhere, including off-site) and **using multiple accounts** both get the post flagged or the account banned. `[VERIFIED https://news.ycombinator.com/newsguidelines.html]`
- **Booster comments from friends.** "Make sure your friends and fans do not add booster comments... HN users are adept at picking up on those, they consider it spamming, and they will flame you." `[VERIFIED https://news.ycombinator.com/showhn.html]`
- **The voting-ring detector** catches coordinated upvotes and prevents the post from hitting the front page. So a Slack/Discord "go upvote" blast is actively counterproductive, not just risky. `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]`
- **User flags** from accounts with 31+ karma reduce ranking or kill the post; a killed post shows `[flagged]` / `[dead]` and is only visible to people with `showdead` on. `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]`
- **New (green) accounts** carry auto-`[dead]` triggers for their first ~2 weeks. If you just made the account to launch, your post can silently die. `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]` Mitigation: use/age an account before launch day. `[INFERRED]`
- Ranking is affected by votes, time, flags, anti-abuse software, overheated-discussion demotion, and moderator action, not votes alone. `[VERIFIED https://news.ycombinator.com/newsfaq.html]`

### 1c. Best day / time

The data does not fully agree, so here are the credible sources and what each found:

- **188,085 Show HN posts, 14 years (the most rigorous public analysis):** best slot is **Monday 00:00 UTC = Sunday 7pm US Eastern**, giving a 10.8% chance of scoring 50+. Median Show HN scores just **2 points**; hitting 50 puts you in the **top 6%**. `[VERIFIED https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/]`
- **23k-post June 2025 analysis:** best odds came from **Sunday, midnight-1am Pacific** — lower competition, decent engagement. `[VERIFIED https://news.ycombinator.com/item?id=44569046]`
- **Practical founder guides** converge on a weekday morning US time (≈9am-12pm Eastern) so you are awake to reply for the next few hours. Treat this as availability planning, not a magic ranking trick. `[VERIFIED https://syften.com/blog/hacker-news-marketing/]`

**Reconciling them:** the off-peak slots (Sun night Pacific) win on *odds* because fewer posts compete. The weekday-morning slot wins on *raw reach* and on your ability to be present. `[INFERRED]` For a first-ever launch where engagement matters more than a vanity peak, pick a window you can sit on for 4 hours. `[OPINION]`

### 1d. The conversion reality (set expectations)

- Each HN upvote converts to roughly **1.4 GitHub stars within 48 hours**. `[VERIFIED https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/]`
- HN score explains only ~8% of the variance in stars (r = 0.29); **comments do not predict stars** (r = 0.10). So a thoughtful thread is for credibility and feedback, not a star multiplier. `[VERIFIED https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/]`
- Show HN volume nearly tripled since 2019 (~28,000 posts in 2025); you compete with ~200 posts/day. The bar is higher than older guides imply. `[VERIFIED https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/]`

### 1e. Comment handling

- **Reply to every comment, positive or negative**, fast and substantively. Visible founder engagement and an interesting thread make readers more likely to upvote. `[VERIFIED https://syften.com/blog/hacker-news-marketing/]`
- Do **not** be defensive on criticism. Concede real points, log them as issues live, link the issue in your reply. `[OPINION]`
- Moderators can rename your title at any time; if yours is too promotional they may neutralize it rather than kill it. `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]`

### 1f. If it flops: the second-chance pool

If the post sinks with little traction, you can email **hn@ycombinator.com** and ask them to consider it for the second-chance pool, which re-surfaces good-but-overlooked links onto the lower front page to retest community interest. Have an email in your HN profile so you get the resubmit notification. Do not spam this. `[VERIFIED https://news.ycombinator.com/item?id=26998309]` `[VERIFIED https://www.pricelevel.com/blog/how-we-leveraged-second-chance-hit-front-page-hacker-news]`

---

## 2. Post-mortems of real comparable launches

### 2a. MemPalace — MCP memory server, viral then corrected (April 2026)

- **What happened:** launched April 5, 2026 with the claim "the highest-scoring AI memory system ever benchmarked." Reported **7,000+ GitHub stars within 48 hours**, trending across AI Twitter, with endorsements from prominent accounts. `[VERIFIED https://explainx.ai/blog/mempalace-local-ai-memory-github]` (Star count reported by secondary coverage, unverified against GitHub's own history.)
- **The correction:** an independent community code review found the headline LongMemEval score ran on raw, uncompressed text using ChromaDB default embeddings — the "memory palace" architecture was **not involved in the benchmark at all**. The "100%" claim was walked back; ~96.6% was the real, mundane number. Maintainers conceded most points. `[VERIFIED https://github.com/lhl/agentic-memory/blob/main/ANALYSIS-mempalace.md]` `[VERIFIED https://www.danilchenko.dev/posts/2026-04-10-mempalace-review-ai-memory-system-milla-jovovich/]`
- **Lessons for you:** (1) A big benchmark claim is the fastest way to attention in this exact niche — and the fastest way to a public unmasking. The audience (r/LocalLLaMA, HN) *will* read your eval code. (2) If you cite numbers, cite them with the methodology inline and link the exact reproduction command, or you become the cautionary tale. This maps directly to your own repo's eval-citation discipline. `[OPINION]`

### 2b. doobidoo/mcp-memory-service — the steady comparable

- **What it is:** open-source persistent memory for AI agents + Claude, MCP server, SQLite-vec + ONNX embeddings, "5ms local reads," claims of 13+ supported tools. This is the closest direct competitor profile to your tool. `[VERIFIED https://github.com/doobidoo/mcp-memory-service]`
- **Traction:** **862 stars, 134 forks** (as reported at time of research). No single viral spike documented; growth looks accretive via README quality + ecosystem listing rather than one big launch. `[VERIFIED https://github.com/doobidoo/mcp-memory-service]` (Reddit/LocalLLaMA discussion threads were not locatable to confirm a specific launch event — treat the "how it grew" as inferred.)
- **Lesson:** a strong README with concrete latency numbers and a broad "works with X, Y, Z tools" matrix is a durable distribution asset even without a viral moment. `[INFERRED]`

### 2c. "Local AI needs to be the norm" — the no-product front-page hit

- A 2025 HN front-page post collected a reported **1,763 upvotes and 800+ comments** with no product, no benchmark, no launch — just a statement that resonated. `[VERIFIED https://dev.to/mininglamp/the-hn-post-that-got-1700-upvotes-local-ai-needs-to-be-the-normwhy-local-ai-just-became-the-32i9]` (Upvote count reported by that writeup, unverified against HN directly.)
- **Lesson:** the "local-first / runs-on-your-machine / no cloud" framing is independently magnetic to the HN + LocalLLaMA crowd. Your tool is local-first; lead with that angle, not "another memory layer." `[OPINION]`

### 2d. Simon Willison's LLM tool-calling launch — the credibility-first model

- Simon shipped LLM 0.26 ("LLMs can run tools in your terminal") and posted **Show HN: My LLM CLI tool can run tools now, from Python code or plugins** on May 31, 2025, paired with a detailed blog post the same day. `[VERIFIED https://news.ycombinator.com/item?id=44110584]` `[VERIFIED https://simonwillison.net/2025/May/27/llm-tools/]`
- Specific upvote/traffic numbers for that thread were not verifiable from the sources gathered — **reported numbers: none confirmed.** What is verifiable is the *pattern*: a plain-language Show HN title, a same-day deep technical writeup as the backing artifact, and an author with an existing audience. `[VERIFIED https://news.ycombinator.com/item?id=44110584]`
- **Lesson:** the launch and a real writeup ship together. The writeup is what gets re-shared after the HN window closes. You have no audience yet, so the writeup has to carry more weight. `[OPINION]`

### 2e. Pattern across all five

The tools that converted attention into installs led with **(a) one-command try-it, (b) a concrete local/offline angle, and (c) honest numbers**. The one that flamed out led with an inflated number that its own audience could disprove. `[INFERRED]`

---

## 3. The multi-channel sequence

Order matters. HN is the anchor and the riskiest (voting-ring detector, flags), so do not pre-blast other channels asking for HN upvotes. `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]` Stagger, do not simultaneously spray.

### Hacker News (anchor)
- Format: `Show HN:` neutral title → repo URL → your own backstory comment. `[VERIFIED https://news.ycombinator.com/showhn.html]`
- Bans/kills: upvote-begging, sockpuppets, booster friends, marketing titles, green-account auto-dead. (See §1b.) `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]`
- Content that wins: try-in-one-command, no signup, a real "what's different" paragraph.

### r/LocalLLaMA
- Etiquette: self-promo is allowed but bounded. Reported community norm: **wait ~30 days between posts about the same project, keep self-promo under ~10% of your activity, and bring real discussion, not just a link.** `[VERIFIED https://www.reddit.com/r/LocalLLaMA/]` (rules surfaced via search summary of the subreddit; confirm the exact current wording in the sidebar before posting — direct fetch was blocked.)
- Content that wins here: local/offline angle, real benchmark numbers *with methodology*, model/embedding details. This audience reads eval code (see MemPalace). Lead with "runs fully local, here's the latency and the eval, here's the repro." `[OPINION]`
- Gets you banned: drive-by link with no body text; benchmark claims you can't reproduce.

### r/ClaudeAI
- **Could not verify the exact current rules** (Reddit fetch + targeted search blocked). General Reddit norm applies: read the sidebar/rules tab and pinned posts first; many subs confine promotion to a weekly thread and require ≥90% non-promotional activity (the "9:1" / 10% rule). `[VERIFIED https://karmaguy.io/en/blog/reddit-self-promotion-rules]` Action: open r/ClaudeAI rules tab manually before posting. `[OPINION]`
- Content that wins: "I made an MCP server that gives Claude Code persistent memory across sessions" with a GIF of it working inside Claude Code. Frame it as a Claude Code workflow win, not a generic memory tool. `[OPINION]`

### r/rust
- Etiquette: showcasing your own Rust project is welcome, but the standard Reddit self-promo norm (~10% of activity, the 9:1 rule) applies; participate first. `[VERIFIED https://karmaguy.io/en/blog/reddit-self-promotion-rules]` (Exact r/rust sidebar wording not directly fetchable here — confirm in the rules tab.)
- There is also **This Week in Rust**, a community newsletter that lists notable projects/posts; getting included is durable, low-effort distribution. `[INFERRED]` Submit via its repo's PR process. `[OPINION]`
- Content that wins: the *engineering* story (libSQL vectors, llama-cpp-2 on Metal, the EventEmitter trait boundary), not the product pitch. r/rust upvotes craft.

### X / Twitter
- Format that wins: a short thread, problem → 30s screen-capture GIF/video → one-line install → repo link. Build-in-public framing. `[OPINION]` (Could not surface a data-backed 2025 source for this; treat as folk wisdom.)
- Bans: no platform rule problem here; the failure mode is being ignored. With no following, X is a multiplier on other channels, not a primary driver. `[INFERRED]`

### Lobsters (lobste.rs)
- Etiquette: **invite-only to post**, and self-promo "should be less than a quarter of one's stories and comments." Tag your own submission with the **`authored`** tag (it has a distinct meaning from `via`). `[VERIFIED https://lobste.rs/about]`
- Gets you banned/flagged: using it as a write-only product-announcement channel; untagged self-promo. `[VERIFIED https://lobste.rs/s/utbyws/mitigating_content_marketing]`
- Content that wins: same engineering-depth post as r/rust. Small, high-signal audience.

### Claude Code plugin marketplace (the highest-fit channel for THIS tool)
- This is where your actual users live. Mechanics:
  - Package the tool as a plugin: a `.claude-plugin/plugin.json` plus your MCP server entry. `[VERIFIED https://code.claude.com/docs/en/discover-plugins]`
  - **You can publish your own marketplace** from a GitHub repo containing `.claude-plugin/marketplace.json`. Users add it with `/plugin marketplace add owner/repo`, then `/plugin install <name>@<marketplace>`. This means you do not need anyone's permission to be installable on day one. `[VERIFIED https://code.claude.com/docs/en/discover-plugins]`
  - To reach the **community marketplace** (`anthropics/claude-plugins-community`), submit via the in-app submission form / the create-plugins guide; entries pass Anthropic's automated validation + safety screening and are pinned to a commit SHA. `[VERIFIED https://code.claude.com/docs/en/discover-plugins]`
  - The **official** `claude-plugins-official` marketplace is curated by Anthropic at their discretion; you cannot self-submit into it. `[VERIFIED https://code.claude.com/docs/en/discover-plugins]`
- Why this matters: the install path is literally `/plugin marketplace add <your repo>` — one line, zero website. Put that exact line at the top of your README and in every post. `[OPINION]`

### Cross-post timing
- Day 1: HN (anchor) + your own writeup goes live. Sit on the thread. `[OPINION]`
- Day 1-2 (a few hours after HN, not simultaneously): r/LocalLLaMA and r/ClaudeAI with channel-tailored bodies. Do NOT link "go upvote my HN" anywhere. `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]`
- Day 2-3: r/rust + Lobsters with the engineering-depth version.
- Continuous: the plugin marketplace listing is permanent; it keeps converting after the launch window's 48h star-impact tail. `[VERIFIED https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/]`

---

## 4. The asset checklist (what separates 5 upvotes from 200)

Have ALL of these done *before* you post. The launch window is hours, not days. `[VERIFIED https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/]`

1. **README above-the-fold.** First 2 lines answer "what is this / why care" in plain words. One-liner ≤10 words. Then logo/badge row (4-7 badges max), then the visual, then quick start. Repos that show the product in the first screenful get materially more stars; reported figure: screenshots correlate with ~42% more stars. `[VERIFIED https://dev.to/iris1031/github-readme-best-practices-how-to-write-a-readme-that-gets-stars-2gb2]` (the 42% figure is from that writeup, unverified original study.)
2. **A 30-second demo GIF/video at the top.** Show the actual loop: Claude Code forgets → install your MCP server → Claude Code remembers across sessions. A visual conveys the value faster than any paragraph. `[VERIFIED https://dev.to/iris1031/github-readme-best-practices-how-to-write-a-readme-that-gets-stars-2gb2]`
3. **The one-line install.** For this tool: `/plugin marketplace add <you>/<repo>` then `/plugin install <name>@<marketplace>`. No signup, no key, no confirmation email. HN penalizes friction. `[VERIFIED https://news.ycombinator.com/showhn.html]` `[VERIFIED https://code.claude.com/docs/en/discover-plugins]`
4. **A social preview image** (GitHub repo → Settings → Social preview) so links unfurl with a real card on X/Reddit/Slack instead of a bare URL. `[INFERRED]`
5. **The honest "ask."** End your HN backstory comment and your Reddit bodies with a specific, modest invitation: "It runs fully local; I'd love feedback on the retrieval quality — here's the eval and the repro command." An honest ask for feedback invites engagement; a hype pitch invites flags. `[VERIFIED https://news.ycombinator.com/showhn.html]`
6. **Numbers with methodology inline.** If you cite eval scores, cite model + dataset + run count + repro command next to the number. This is exactly what MemPalace failed to do. `[VERIFIED https://github.com/lhl/agentic-memory/blob/main/ANALYSIS-mempalace.md]`
7. **An aged, real HN account** (not freshly created, not named after the project). `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]` `[VERIFIED https://news.ycombinator.com/showhn.html]`
8. **Issues triage ready.** Be able to open and link GitHub issues live as feedback lands.

**What makes it 5 vs 200:** the 200-upvote launches let a stranger go from "interesting" to "it's running on my machine" in under a minute, and the author was in the thread converting skeptics. The 5-upvote launches buried the demo, required signup, used a salesy title, or the author posted and walked away. `[INFERRED from §1, §2, §4]`

---

## 5. Launch-day runbook (hour by hour)

Pick a day you can be at the keyboard for ~6 hours. Default below assumes a US-morning launch for presence; if you optimize for odds instead, use Sunday ~7pm ET / Sun midnight-1am PT and front-load the prep the day before. `[VERIFIED https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/]` `[VERIFIED https://news.ycombinator.com/item?id=44569046]`

**T-1 day**
- Finalize README, GIF, social preview, one-line install. Test the install on a clean machine. `[VERIFIED https://news.ycombinator.com/showhn.html]`
- Publish your writeup as a draft (blog/GitHub page). It backs the launch. `[VERIFIED https://simonwillison.net/2025/May/27/llm-tools/]`
- Confirm your HN account is aged and has an email set (for second-chance notification). `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]` `[VERIFIED https://news.ycombinator.com/item?id=26998309]`
- Publish the plugin marketplace repo so `/plugin marketplace add` already works. `[VERIFIED https://code.claude.com/docs/en/discover-plugins]`

**Hour 0 — post to HN**
- Title: `Show HN: <Tool> – local-first memory for Claude Code (MCP, runs offline)`. Neutral, no exclamation. URL field = repo. Text box blank. `[VERIFIED https://news.ycombinator.com/showhn.html]`
- Immediately add your backstory comment: why you built it, what's different, the one-line install, the honest feedback ask. `[VERIFIED https://gist.github.com/tzmartin/88abb7ef63e41e27c2ec9a5ce5d9b5f9]`
- Do NOT message anyone to upvote. `[VERIFIED https://news.ycombinator.com/newsguidelines.html]`

**Hours 0-3 — the only window that really matters**
- Refresh the thread every few minutes. Reply to every comment, fast, concrete, non-defensive. `[VERIFIED https://syften.com/blog/hacker-news-marketing/]`
- Concede valid criticism and open issues live; link them in replies. `[OPINION]`
- This is where 92%-of-impact-in-48h plays out; the first hours set the trajectory. `[VERIFIED https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/]`

**Hours 2-4 — second channel (only after HN has settled in, not simultaneously)**
- Post to r/LocalLLaMA: local/offline framing + real eval numbers with repro + GIF. Confirm the sidebar rules first. `[VERIFIED https://karmaguy.io/en/blog/reddit-self-promotion-rules]`
- Post to r/ClaudeAI: Claude Code workflow framing + GIF. **Read its rules tab first** (could not verify remotely). `[OPINION]`
- Keep these as genuine posts with discussion bodies, never "upvote my HN." `[VERIFIED https://github.com/minimaxir/hacker-news-undocumented]`

**Hours 4-8**
- Keep answering HN + Reddit. Post the X thread (problem → GIF → install → repo). `[OPINION]`
- Tweet/Reddit-reply to anyone who installs and reports back.

**Day 2**
- Post to r/rust and Lobsters with the engineering-depth angle (libSQL vectors, llama-cpp-2/Metal, crate boundaries). Tag Lobsters with `authored`. `[VERIFIED https://lobste.rs/about]`
- Submit to This Week in Rust if relevant. `[OPINION]`
- Keep triaging issues; ship one or two quick fixes from launch feedback and reply to the people who reported them.

**Day 2-3 — if HN flopped**
- Email hn@ycombinator.com asking them to consider it for the second-chance pool. One polite ask, no spam. `[VERIFIED https://news.ycombinator.com/item?id=26998309]`

**Ongoing**
- The plugin-marketplace listing keeps converting after the 48h window. Respect the ~30-day / 10% self-promo cadence before re-posting any single subreddit. `[VERIFIED https://karmaguy.io/en/blog/reddit-self-promotion-rules]`

---

## Source list

- Show HN official rules — https://news.ycombinator.com/showhn.html
- HN guidelines — https://news.ycombinator.com/newsguidelines.html
- HN FAQ (ranking factors) — https://news.ycombinator.com/newsfaq.html
- Show HN submission checklist — https://gist.github.com/tzmartin/88abb7ef63e41e27c2ec9a5ce5d9b5f9
- Show HN by the Numbers (188k posts, conversion + timing) — https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/
- When to Post on HN (23k posts, June 2025) — https://news.ycombinator.com/item?id=44569046
- HN posting guide (timing, comment handling) — https://syften.com/blog/hacker-news-marketing/
- hacker-news-undocumented (green accounts, flags, voting-ring, dead) — https://github.com/minimaxir/hacker-news-undocumented
- Second-chance pool — https://news.ycombinator.com/item?id=26998309
- Second-chance pool (case study) — https://www.pricelevel.com/blog/how-we-leveraged-second-chance-hit-front-page-hacker-news
- MemPalace viral launch — https://explainx.ai/blog/mempalace-local-ai-memory-github
- MemPalace independent code review — https://github.com/lhl/agentic-memory/blob/main/ANALYSIS-mempalace.md
- MemPalace benchmark correction — https://www.danilchenko.dev/posts/2026-04-10-mempalace-review-ai-memory-system-milla-jovovich/
- doobidoo/mcp-memory-service — https://github.com/doobidoo/mcp-memory-service
- "Local AI needs to be the norm" (1,763 upvotes, reported) — https://dev.to/mininglamp/the-hn-post-that-got-1700-upvotes-local-ai-needs-to-be-the-normwhy-local-ai-just-became-the-32i9
- simonw LLM tools Show HN — https://news.ycombinator.com/item?id=44110584
- simonw LLM tools writeup — https://simonwillison.net/2025/May/27/llm-tools/
- Lobsters about/rules — https://lobste.rs/about
- Lobsters content-marketing norms — https://lobste.rs/s/utbyws/mitigating_content_marketing
- Claude Code plugin marketplaces — https://code.claude.com/docs/en/discover-plugins
- Reddit self-promotion rules (9:1 / 10%) — https://karmaguy.io/en/blog/reddit-self-promotion-rules
- README best practices — https://dev.to/iris1031/github-readme-best-practices-how-to-write-a-readme-that-gets-stars-2gb2
- Launch-Day Diffusion (arxiv, HN→GitHub stars; abstract only, full text not fetched) — https://arxiv.org/abs/2511.04453

### Verification notes / gaps
- Reddit (r/LocalLLaMA, r/ClaudeAI, r/rust) and several blog hosts blocked direct fetch; their rules are reported via search-summary or general Reddit self-promo norms. Confirm exact current sidebar wording manually before posting.
- MemPalace "7,000+ stars in 48h" and "Local AI needs to be the norm" 1,763 upvotes are reported by secondary write-ups, not independently confirmed against GitHub/HN.
- No specific upvote/traffic number for simonw's launch thread could be verified — reported numbers: none confirmed.
- The arxiv "Launch-Day Diffusion" paper's full numbers could not be fetched; the conversion figures here come from the danfking 188k-post analysis instead.
