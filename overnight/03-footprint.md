# Origin (useorigin.app / 7xuanlu) — Public Footprint & Traction Scorecard

Investigation date: 2026-05-31. Goal: an honest, evidence-based read on external traction.
Hypothesis going in was "almost none." The data supports that.

Every number below is tagged. [VERIFIED url] = pulled from a fetched source.
[INFERRED] = reasoned from verified facts. [ESTIMATE] = math shown. [OPINION] = my read.

## Namespace warning (read first)

There are two unrelated "Origin"s and they collide hard in search:

- **useorigin.com** — a well-funded, SEC-regulated AI financial advisor (budgeting app,
  iOS/Android, press on Yahoo Finance). NOT this project. It owns the "Origin" namespace
  on Google. [VERIFIED https://finance.yahoo.com/news/origin-unveils-first-ai-financial-140000884.html]
- **useorigin.app** — this project. A solo local-first AI-memory daemon by 7xuanlu.

Almost every "Origin" search result is the financial-advisor company. The memory project
surfaces only via its own GitHub repo and auto-generated package-aggregator pages. That
namespace collision is itself a discoverability problem for this project. [OPINION]

## Traction Scorecard

| Channel | Metric | Number | Source |
|---|---|---|---|
| GitHub `7xuanlu/origin` | Stars | **34** | [VERIFIED github API repo search, also github.com/7xuanlu/origin] |
| GitHub `7xuanlu/origin` | Forks | **2** | [VERIFIED github API] |
| GitHub `7xuanlu/origin` | Watchers | 34 (API field) / "0" on web header | [VERIFIED github API repo search; web header rendered 0] |
| GitHub `7xuanlu/origin` | Open issues | **9** | [VERIFIED github API] |
| GitHub `7xuanlu/origin` | Repo age | created 2026-04-19 (~6 weeks) | [VERIFIED github API created_at] |
| GitHub `7xuanlu/origin` | External issue authors | **1** (kiluazen / "kushal", issue #194) | [VERIFIED github list_issues; all others authored by 7xuanlu] |
| GitHub `7xuanlu/origin` | Total open issues by non-owner | 1 of 4 listed | [VERIFIED github list_issues] |
| GitHub `7xuanlu/origin` | Commit authors (last 100 commits) | **1** — 100/100 are 7xuanlu | [VERIFIED github list_commits, tallied] |
| GitHub `7xuanlu/origin` | Releases shipped | 18 tags v0.1.0 → v0.7.0 | [VERIFIED github list_releases] |
| GitHub `7xuanlu/origin` | Release binary download counts | not retrievable via tools used | [INFERRED — assets array not exposed; see gaps] |
| GitHub `7xuanlu/origin-mcp` | Stars / state | **1 star, archived** | [VERIFIED github API repo search] |
| GitHub `7xuanlu/origin-app` | Stars | **0** (created 2026-05-07) | [VERIFIED github API] |
| GitHub `7xuanlu/origin-website` | Stars | **0** | [VERIFIED github API] |
| GitHub other `7xuanlu/origin-*` repos | Stars | origin-legacy 1, origin-mcp-legacy 0, origin-plugin 0 (archived), origin-new-empty 0 | [VERIFIED github API] |
| GitHub profile top non-Origin repos | Stars | thinkord-origin 21, ncu-gpa-calculator 9 (both 2019-2020, stale) | [VERIFIED github API] |
| npm `origin-mcp` | Versions published | 12 (0.1.0 → 0.7.0) | [VERIFIED https://registry.npmjs.org/origin-mcp] |
| npm `origin-mcp` | First publish | 2026-04-19 | [VERIFIED registry.npmjs.org] |
| npm `origin-mcp` | Latest publish | 2026-05-25 (v0.7.0) | [VERIFIED registry.npmjs.org] |
| npm `origin-mcp` | Sole maintainer | h164654156465@gmail.com (the project owner) | [VERIFIED registry.npmjs.org] |
| npm `origin-mcp` | Weekly/monthly downloads | **NOT OBTAINED** — api.npmjs.org blocked in this env | [see gaps — no number, not fabricated] |
| npm `@7xuanlu/origin` | Download stats | **NOT OBTAINED** — api blocked | [see gaps] |
| useorigin.app | Site content / funnel | **NOT OBTAINED** — 403 on fetch, no search snippets | [see gaps] |
| Hacker News | Posts mentioning origin-mcp / useorigin / 7xuanlu | **0** | [VERIFIED hn.algolia search + site:news.ycombinator.com search — only unrelated memory-MCP projects returned] |
| Reddit | Mentions | **0 found** | [VERIFIED WebSearch — no reddit results for this project] |
| Product Hunt | Listing | **0 found** | [VERIFIED WebSearch] |
| Lobsters | Mentions | **0 found** | [VERIFIED WebSearch] |
| X / Twitter | Mentions | **0 found** | [VERIFIED WebSearch — none surfaced] |
| News / blogs (independent) | Coverage | **0** | [VERIFIED WebSearch — only the unrelated useorigin.com financial company] |
| Auto-aggregator listings | Presence | lib.rs/crates/origin-mcp, mcpmarket.cn | [VERIFIED WebSearch — mirror pages, not traction] |
| YouTube demo youtu.be/k37gjWVPHwI | View count | **NOT OBTAINED** — 403 on fetch | [see gaps] |

## What could not be measured (honest gaps, no guessing)

- **npm download counts.** Every attempt at api.npmjs.org (point + range, last-day/week/month,
  both packages) returned 403 in this sandbox, and npm-stat.com / npmjs.com package pages also
  403'd. WebSearch surfaced no third-party snapshot of the number. So the single most direct
  "real users" signal is unmeasured here. I refuse to invent a figure. To get it, run from an
  unrestricted network: `curl https://api.npmjs.org/downloads/point/last-month/origin-mcp`.
- **useorigin.app site content.** The domain 403'd on every fetch and yields zero search
  snippets (the namespace is swamped by useorigin.com). Could not assess headline, funnel,
  waitlist, /docs, or /learn directly. The GitHub README does link out to useorigin.app.
  [VERIFIED github.com/7xuanlu/origin]
- **GitHub release binary download counts.** The release objects I pulled don't expose the
  per-asset `download_count`. Unmeasured.
- **YouTube view count.** youtu.be 403'd; not visible.

## The honest read

The hypothesis ("almost none") holds. On every channel that is publicly verifiable, the
external footprint is at or near zero:

1. **It is a solo project.** 100 of the last 100 commits are 7xuanlu's. The npm maintainer is
   the owner's personal Gmail. No co-maintainers, no external committers. [VERIFIED]

2. **One external human has engaged with the repo at all** — GitHub user `kiluazen` filed a
   single thoughtful design issue (#194) comparing Origin's provenance model to another project.
   That is the entire externally-sourced issue traffic. Every other issue is the author talking
   to himself (roadmap/bug notes). [VERIFIED]

3. **34 stars on a 6-week-old repo, 2 forks, ~1 watcher** is hobby-project territory, not
   traction. For comparison, the competing memory-MCP projects that DO show up on Hacker News
   (ContextForge, Ember MCP, Agent Recall, OpenTimelineEngine) each got a Show HN; Origin has
   none. [VERIFIED hn.algolia]

4. **Zero distribution events.** No Show HN, no Reddit thread, no Product Hunt launch, no
   Lobsters post, no press, no blog coverage by anyone other than the author. The only non-GitHub
   pages are passive aggregator mirrors (lib.rs, mcpmarket.cn) that index every package
   automatically and signal nothing about usage. [VERIFIED WebSearch + hn.algolia]

5. **The brand is fighting a losing namespace battle.** "Origin" + "useorigin" both resolve to
   a funded SEC-regulated fintech. A solo dev on the `.app` TLD has no chance of out-ranking that
   organically. Discoverability is structurally capped until the project either renames or earns
   inbound links from a launch. [OPINION]

Bottom line: the engineering surface is large and active (18 releases, 5 crates, cross-platform,
eval harness) but it is **all supply, no demand**. The shipping velocity is real; the audience,
on every measurable public channel, is not yet there. The one genuinely external signal is a
single GitHub issue from one interested developer. Everything else is the author's own output or
automated mirrors.

Caveat: the npm download count is the one number that could move this read, and it is the one
number this environment could not fetch. If monthly installs are in the thousands, the picture
softens from "no traction" to "quiet install base, no community." Pull that number before any
external claim. Given 1 star on the npm-published `origin-mcp` repo and zero social footprint,
[OPINION] a low number is the more likely outcome, but that is inference, not measurement.
