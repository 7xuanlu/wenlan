# README visual assets

## Preserved concept illustrations

The Agent Loop overview is retained for future reuse, although the main READMEs now use the approved compositions below in its place:

- `wenlan-system.svg` / `.png`: English desktop overview.
- `wenlan-system-zh-Hant.svg` / `.png`: Traditional Chinese desktop overview.
- `wenlan-system-zh-Hans.svg` / `.png`: Simplified Chinese desktop overview.
- Matching `*-mobile.svg` / `.png` versions remain alongside them.

The original knowledge-network illustrations (`wenlan-knowledge-network*`) are also retained. These are explanatory illustrations, not captures of the app.

Their editable source is `scripts/readme-product-visuals.cjs`; the renderer and font setup are documented in [the visual toolchain](../../scripts/readme-visuals/README.md). Removing an illustration from the README does not retire its assets or generator.

## Current product captures

See [screenshots and provenance](screenshots/README.md). Product captures use fictional content and the actual frontend, without compositing UI elements.

## Approved README compositions

The AI-work illustration is fully localized for English, Traditional Chinese, Simplified Chinese, and Spanish, including the Claude conversations, retrieval cards, graph labels, and finished homepage. The panorama, personal graph, and update-history compositions now have English, Traditional Chinese, and Simplified Chinese versions. Their approved composition, demo content, nodes, and relationships are retained. Localized versions translate captured text using the original DOM/styles and text layers; they are localized composites, not newly captured native-language UI. Spanish versions of these three figures are deferred.

The final consistency pass aligns the three explanatory titles (46px, semibold, shared left edge), preserves native type icons and colors, and moves provenance footnotes into readable README captions. It preserves approved body composition and content. The video cover uses a shortened demo article captured in the App, an enlarged native memory preview, and a larger native directory crop. Graph labels and spacing are balanced for README width; provenance stays in the caption beneath the image.

| Asset | Export | Provenance |
| --- | --- | --- |
| `wenlan-personal-knowledge.png`, `-zh-Hant.png`, `-zh-Hans.png` | 6000 × 3600 | Actual App directory/header captures and a high-resolution render of the actual editor and memory-preview DOM/styles, combined with an editorial graph layout. All 33 graph nodes and 80 typed relationships retain their verified demo identities. Not a native three-column screen; App behavior is unchanged. |
| `wenlan-personal-graph.png`, `-zh-Hant.png`, `-zh-Hans.png` | 3600 × 2488 | Actual graph capture; the original nodes and connections are preserved, with localized legend and text layers. Region names and boundaries are editorial annotations, not verified source-node identities or detected communities. |
| `wenlan-knowledge-to-work.png`, `-zh-Hant.png`, `-zh-Hans.png`, `-es-ES.png` | 3600 × 2320 each | Fictional Claude conversations and FORMA homepage, using the film’s unchanged hero-image asset. Keyword, semantic, and graph paths point to the matching headline, primary action, and image. Concept illustration, not a measured comparison; Wenlan context complements Claude’s native capabilities. |
| `wenlan-updates-history.png`, `-zh-Hant.png`, `-zh-Hans.png` | 3600 × 2000 | Actual memory, source, page, and revision captures with demo data; connecting lines and editorial labels are illustrative. Shows automatic updates to untouched AI-organized pages and per-page review for user-created or edited pages. |

The graph uses the website's September 6, 2026 capture, `wenlan-live-knowledge-graph-20260906.webp` (source SHA-256 `63ab14b444c9cfd8527f9fc80440d7fb2507856b745c1d83b10e52e61df0b5f9`). Original graph nodes, colors, and connections remain intact. Urban history, Genetics and evolution, and Human–AI collaboration research are illustrative region names.

The localization sources are retained in the `readme-locales-final` production package. Panorama exports are 6000 × 3600; graph exports are 3600 × 2488; update-history exports are 3600 × 2000. Each was inspected at 900px reading width. The Traditional Chinese panorama remains byte-identical to its approved version; its graph changes only the native English legend and footer text to Traditional Chinese. Longer translations wrap inside the existing composition.

Composition sources: `cover-polish-title` (6000px `cover-polish-hd` master; approved `cover-polish-v2` layout), `real-graph-polish-v7`, `ai-work-film-case-v7` / `ai-work-film-case-v7-localized`, and `lifecycle-polish-v26`, retained in their corresponding production packages, including `cover-polish-title`, `cover-polish-hd`, `readme-final-consistency`, and `ai-work-film-case-v7-localized`. The final title pass reduces only the article heading from 28.16px to 26px; every pixel outside that heading area is identical to the 6000px master. The cover package includes read-only snapshots of the actual editor and hover-preview DOM/styles, original pseudo-element icon CSS, the approved composition, export instructions, and SHA-256 verification. Text and graph labels are rendered at high resolution rather than enlarged from the former small article crop. Heading font fallback is normalized to match the approved capture. Directory/header raster inputs remain unchanged; their detail is limited by the original captures. Browser capture uses JPEG transport; the published PNG adds no further lossy encoding. No generative enhancement or invented UI content is used in the product cover. The AI-work comparison remains an authored concept as noted above.

The older `wenlan-lifecycle*` and `wenlan-knowledge-network*` illustrations and their generators remain available for reference; the current READMEs do not use their old mobile variants.

### Current export hashes

- `wenlan-personal-knowledge-zh-Hant.png`: `328e4bf032d5dcf16cad24658d7d12a51d694fdd2b3d00f5b1e44f94b22246e5`
- `wenlan-personal-graph-zh-Hant.png`: `65a4588af858c942060477578ef2340a0ff967bf6bebde67ebf6d8de66aafc97`
- `wenlan-knowledge-to-work-zh-Hant.png`: `3327dca95845e111d577fdc3d823b659ed41b5705d96ccb0d236125e14b23777`
- `wenlan-knowledge-to-work-zh-Hans.png`: `030eaf6323dbb884265065bb8490c83f3dacad9007e934512fb1708064f5a8e1`
- `wenlan-knowledge-to-work.png`: `d528201d28ddf71c9503e96d4ecbd8f327630460412c00815239e14d76d11d02`
- `wenlan-knowledge-to-work-es-ES.png`: `2b36d4e24b9c0e5765d951af1f1bc55d5a76aa1e9b8551615a5843568d0d316d`
- `wenlan-updates-history-zh-Hant.png`: `dd957457cb3dcd4bbc8303041aa56a45005f7931e1df94aabc1a2028662b09ea`
- `wenlan-personal-knowledge.png`: `6aa9bd21bb7e5b746ec1c1545570f0f5df9539fb81ffcf4dc1f915295b2de0e9`
- `wenlan-personal-knowledge-zh-Hans.png`: `96736638a3611964a0b63888e41777f98d07f93e8f9db7b39c964df43dc4eeba`
- `wenlan-personal-graph.png`: `a06050ed7a3a5bb3d43ef31e5a7982b9a640ac328167487b3490bb5da70cccf2`
- `wenlan-personal-graph-zh-Hans.png`: `46330eb6094ca49b149bf05acc6d234d93b5ca3b14d9574fe72d5b9c23607696`
- `wenlan-updates-history.png`: `35ca643f069e2cbbfb91c1570b250948ae4aa1fa8bdb4fc7e382cf53d3519259`
- `wenlan-updates-history-zh-Hans.png`: `ffd16c6bd03c6a3b2810fade591b918f0cc068592b1d9bbd1d4aba90bb9480a6`

## Neutral background palette

The daily workflow, update/history, architecture, and AI-work compositions use the approved neutral background direction: `#FCFCFC` canvas, neutral `#F4F5F5` panels, and white cards. Warm editorial background fills are neutralized; existing text, accents, semantic highlights, and native App screenshot colors are retained. The already-neutral panorama and personal graph remain byte-identical, as does the dark banner.

The palette pass preserves the approved PNG dimensions and original text/geometry pixels. Background masks come from before/after renders of the existing code-native sources; only matching light background pixels are changed, without resampling the approved raster. All 11 native capture regions in the Traditional Chinese history composition are pixel-identical. Four-language variants were inspected at 900px README width; Spanish retains the previously approved fallback images where localized artwork was deferred. Source, masks, original exports, and verification are retained in the `readme-neutral-20261017` production package. The earlier `shared-visual-palette-20261016` package remains the history composition's source; its compact heading and warm-orange paths are unchanged.

## Architecture overview

`wenlan-architecture.png`, `-zh-Hant.png`, `-zh-Hans.png`, and `-es-ES.png` are fully localized architecture illustrations, exported at 4800 × 3300. Matching SVG files are the editable vector sources. They are diagrams, not App screenshots. The approved Traditional Chinese composition is preserved; longer translated labels use concise wording and local line wrapping.

The diagram separates imported materials, App/CLI and AI-tool access, the local service, and persistent knowledge. It distinguishes local search models, the external AI client’s own model, and optional background enrichment models. Page citations, dependent-page invalidation, review for human-owned pages, and version history are represented at product level, not as a complete database schema or a guarantee that every retrieval uses every channel.

Source references: [setup and model roles](../setup-and-data.md#models), [retrieval and connected knowledge](../technical-foundations.md), [source invalidation](../../crates/wenlan-core/src/db.rs), and [Page ownership and proposed revisions](../../crates/wenlan-core/src/post_write/page_update.rs). Rendered at 3× from SVG and checked at 900px README width. Existing approved product figures are unchanged.

## Daily workflow

`wenlan-daily-workflow.png`, `-zh-Hant.png`, `-zh-Hans.png`, and `-es-ES.png` are fully localized concept diagrams, exported at 3984 × 2256. Matching SVG files preserve editable text and geometry. The Traditional Chinese export is the approved V13 master; other locales retain its layout, icons, colors and connections, with localized line wrapping.

Sources and existing memories enter a shared local library. App access is bidirectional. AI tools read shared knowledge through a plugin/MCP connection and save memories, session records and progress back. `/capture`, `/handoff`, and `/brief` are optional actions in AI work, not a mandatory sequence. Handoff does not imply automatic transcript import or page synthesis. Knowledge-page organization and updating remain optional.

Source contracts: [capture](../../plugin/skills/capture/SKILL.md), [handoff](../../plugin/skills/handoff/SKILL.md), [brief](../../plugin/skills/brief/SKILL.md), and [knowledge guide](../knowledge-guide.md). All four locale exports were inspected at 830px reading width. The existing README images are unchanged.
