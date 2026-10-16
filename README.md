<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/readme-banner-mobile.png">
    <img src="./docs/assets/readme-banner.png" alt="Wenlan: A living personal wiki. AI organizes. You stay in control." width="100%">
  </picture>
</p>

<p align="center">
  Wenlan organizes scattered notes, documents, and conversations into a knowledge base that stays current and links back to its sources,<br>
  so you and AI tools like Claude and Codex can use it to find answers and get work done.
</p>

<p align="center">
  English | <a href="./README.zh-Hans.md">简体中文</a> | <a href="./README.zh-Hant.md">繁體中文</a> | <a href="./README.es-ES.md">Español</a>
</p>

<p align="center">
  <a href="https://github.com/wenlan-app/wenlan/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/wenlan-app/wenlan/actions/workflows/ci.yml/badge.svg?branch=main&event=push"></a>
  <a href="https://github.com/wenlan-app/wenlan/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/wenlan-app/wenlan?sort=semver&label=release"></a>
  <a href="#license"><img alt="License: Apache-2.0 and AGPL-3.0" src="https://img.shields.io/badge/license-Apache--2.0%20%2B%20AGPL--3.0-blue.svg"></a>
</p>

<p align="center">
  <a href="#start-in-30-seconds">Get&nbsp;started</a> ·
  <a href="#what-can-i-bring-in">Capabilities</a> ·
  <a href="#learn-more">Docs</a> ·
  <a href="https://discord.gg/nmzVTTU3HG">Discord</a>
</p>

<!-- Replace the linked walkthrough when the separately produced release video is ready. -->
<p align="center">
  <a href="https://github.com/user-attachments/assets/35f06749-00e5-484d-a9f4-5e462de8d11e">
    <img src="./docs/assets/wenlan-personal-knowledge-zh-Hant.png" alt="Connected pages, memories, and sources in Wenlan" width="100%">
  </a>
  <br><a href="https://github.com/user-attachments/assets/35f06749-00e5-484d-a9f4-5e462de8d11e">▶ Watch the video</a>
</p>
<p>Composite of App views with demo data; the graph is illustrative, not a native three-column layout.</p>

<a id="quickstart"></a>
<a id="start-in-30-seconds"></a>
<a id="start-with-the-app"></a>
<a id="open-the-wiki"></a>
<a id="desktop-app"></a>
<a id="claude-code-in-30-seconds"></a>
<a id="codex-plugin"></a>
<a id="mcp-setup"></a>
<a id="mcp-clients"></a>

## Get started

### 1. Download and open Wenlan

[Download the desktop app](https://github.com/wenlan-app/wenlan/releases/latest), then open it:

- **macOS (Apple Silicon):** Open the `.dmg` and drag Wenlan to Applications. The app is signed and notarized.
- **Windows x64:** Run the `-setup.exe`; it is not signed yet. If SmartScreen warns you, confirm it came from the official Releases page before choosing “More info” → “Run anyway.”
- **Linux:** There is no desktop app yet; use the [AI setup guide](docs/setup-with-ai.md#install-the-runtime) to install the runtime.

First launch downloads the local search model. Keep an internet connection until setup finishes. [Download and privacy details](docs/PRIVACY.md#when-wenlan-reaches-the-network).

### 2. Open a sample or create a page

Choose **Open notes** to enter Wenlan. Explore the sample from the welcome screen or create a page of your own.

### 3. Connect an AI tool when you want

Connect Claude Code, Codex, or another supported client when you want it to use your saved knowledge. Follow the [AI setup guide](docs/setup-with-ai.md).

> Use my earlier notes to help me finish this task.

<a id="what-does-wenlan-build"></a>
<a id="why-it-compounds"></a>
<a id="what-is-this"></a>
<a id="what-you-get"></a>
<a id="what-can-it-do"></a>
<a id="what-can-i-bring-in"></a>
<a id="knowledge-graph"></a>

<a id="browse-and-organize-your-knowledge"></a>

<a id="connect-your-knowledge"></a>

## Organize scattered material into knowledge

Edit **Markdown Pages with citations** and link related content through `[[wikilinks]]`. Follow a citation to check the original document, conversation, or saved Memory behind an idea.

- **File and conversation imports:** Import Markdown, text, text-based PDFs, folders, and ChatGPT or Claude export ZIPs while retaining source references. Scanned PDFs need text extraction before import.
- **Read-only Obsidian connection:** Read an existing vault and refresh it on demand without modifying its files or syncing changes back.
- **Quick Capture and atomic Memories:** Save one decision, lesson, or preference at a time in the app or through your AI, with its source.
- **Knowledge graph:** Discover related topics through typed Entities, relations, and knowledge clusters (communities); a configured enrichment model links repeated mentions to the same entity (mention resolution).

<a id="explore-your-personal-knowledge-graph"></a>

<p align="center">
  <img src="./docs/assets/wenlan-personal-graph-zh-Hant.png" alt="Wenlan knowledge graph with illustrative regions for personal interests, research, and development" width="100%">
</p>
<p>App nodes and connections; region names and boundaries are illustrative.</p>

<a id="how-wenlan-works"></a>
<a id="how-does-it-work"></a>

<a id="what-wenlan-is-not"></a>
<a id="retrieval"></a>
<a id="retrieval-across-words-meaning-and-connections"></a>

<a id="use-your-knowledge-with-ai"></a>

## Put your knowledge to work with AI

Bring earlier findings and decisions into your next writing, research, or development task. Through **MCP and plugins**, Claude Code, Codex, and other connected AI tools can retrieve relevant knowledge and save new lessons to the same local knowledge base.

- **[Local hybrid retrieval](docs/technical-foundations.md#retrieval-pipeline):** Find exact terms and similar meaning with SQLite FTS5 and BGE embeddings, fused by weighted Reciprocal Rank Fusion (RRF); base search needs no API key.
- **Graph-assisted ranking:** Rank related Memories higher when Entity links are available, with optional cross-encoder reranking to refine relevance.
- **Plugin workflow:** Find context with `/recall`, save decisions with `/capture`, synthesize Pages with `/distill`, and leave progress and open questions with `/handoff` ([commands](plugin/skills/README.md)).
- **Space-scoped retrieval:** Select a Space to limit returned knowledge to the relevant project or personal collection.

<p align="center">
  <img src="./docs/assets/wenlan-knowledge-to-work-zh-Hant.png" alt="Let AI continue from what you already know: keyword, semantic and Graph retrieval" width="100%">
</p>
<p>Fictional example, not a product test; Wenlan adds context alongside Claude’s native memory.</p>

Experimental web access lets supported web clients search the whole library or one chosen Space while the computer running Wenlan stays online. Queries and results pass through a relay; see the [privacy limits](docs/PRIVACY.md#pre-release-standalone-wenlan-relay-connector).

<a id="what-makes-wenlan-distinct"></a>
<a id="why-is-wenlan-different"></a>
<a id="two-lifecycles"></a>
<a id="two-lifecycles-one-maintained-knowledge-system"></a>
<a id="local-markdown"></a>
<a id="atomic-memory"></a>
<a id="maintained-page"></a>
<a id="local-markdown-that-works-with-obsidian"></a>

<a id="keep-knowledge-current-and-preserve-history"></a>

## Keep knowledge current, with its history

When new material changes an earlier conclusion, Wenlan can refresh the affected Pages. **Evidence tracking** records their supporting Sources and Memories.

- **Steep background enrichment:** With a [configured model](docs/setup-and-data.md#models), prepare captured Memories for reuse with types, structured details, search cues, and graph links; Page synthesis is optional.
- **[Revision review and history](docs/knowledge-guide.md#updates-and-history):** Compare Page versions and trace Memory replacements; automatic refresh updates eligible machine-maintained Pages directly and proposes revisions for Pages you edited.
- **Local Markdown and wiki export:** Read Pages as ordinary files and export eligible Pages as an Open Knowledge Format (OKF) wiki; Memories and graph data stay in local libSQL.
- **Choose the model runtime:** Run enrichment and synthesis with on-device Qwen (llama.cpp), local OpenAI-compatible servers such as Ollama or LM Studio, or configured cloud providers.

<p align="center">
  <img src="./docs/assets/wenlan-updates-history-zh-Hant.png" alt="Illustrative flow: a finding in a source document updates memory from V1 to V2, then informs a knowledge page that is either updated automatically or sent for review." width="100%">
</p>
<p>App views with demo data; connections and labels are illustrative.</p>

Automatic refresh rejects drafts without enough citations; direct file edits and forced regeneration follow different review rules.

<a id="models-and-privacy"></a>

## Data and privacy

A cloud AI client may send retrieved knowledge to its provider, and a cloud model receives the material it processes. Local storage does not keep those interactions local. Usage statistics are off by default.

[File locations, backup, and removal](docs/setup-and-data.md#backup-and-removal) · [Network and privacy details](docs/PRIVACY.md#when-wenlan-reaches-the-network).

<a id="evaluation"></a>

## Evaluation

These results show whether Wenlan search finds related saved knowledge; they do not measure the quality of AI answers.

<!-- EVAL_SNAPSHOT_START -->
| Benchmark | Recall@5 | MRR | NDCG@10 |
|---|---:|---:|---:|
| LME_Oracle (500 Q) | 93.6% | 0.857 | 0.883 |
| LME_S (deep, 90 Q) | 87.7% | 0.815 | 0.822 |
<!-- EVAL_SNAPSHOT_END -->

[Evaluation method and benchmark details](docs/eval/README.md).

<a id="learn-more"></a>

<a id="offline-queue-outbox"></a>

## Learn more

- [Get started and install](docs/setup-and-data.md#installation)
- [Connect an AI tool](docs/setup-with-ai.md)
- [Sources and imports](docs/knowledge-guide.md#sources-and-pages) · [Local files, backups, and offline use](docs/setup-and-data.md)
- [Privacy](docs/PRIVACY.md)
- [Doctor and issue reports](https://wenlan.app/docs/diagnostics-and-issue-reports)
- [Full documentation index](docs/README.md)

## Contributing

Join the [Discord community](https://discord.gg/nmzVTTU3HG). Report bugs or request features in [GitHub issues](https://github.com/wenlan-app/wenlan/issues); issues are public, so redact secrets and personal information. For code and docs, see [Contributing](.github/CONTRIBUTING.md), [Security](.github/SECURITY.md), and the [Code of Conduct](.github/CODE_OF_CONDUCT.md).

<a id="code-signing-policy"></a>

## Code signing policy

See the [platform signing and notarization details](docs/code-signing.md).

<a id="license"></a>

## License

The local runtime, CLI, MCP server, shared types, and plugins use [Apache-2.0](LICENSE). The desktop app and its frontend use [AGPL-3.0-only](app/LICENSE); modified versions run as a network service must offer their source to users.

<a id="acknowledgments"></a>

## Lineage and peers

Wenlan takes its name from Wenlan Pavilion. Its direction builds on the LLM-wiki and agent-memory lineages: [Karpathy’s LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f), [Rohitg00’s proposal](https://gist.github.com/rohitg00/2067ab416f7bbe447c1977edaaa681e2), and [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki).
