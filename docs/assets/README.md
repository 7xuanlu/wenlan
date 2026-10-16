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
| `wenlan-updates-history.png`, `-zh-Hant.png`, `-zh-Hans.png` | 3600 × 2080 | Actual memory, source, page, and revision captures with demo data; connecting lines and editorial labels are illustrative. Shows automatic updates to untouched AI-organized pages and per-page review for user-created or edited pages. |

The graph uses the website's September 6, 2026 capture, `wenlan-live-knowledge-graph-20260906.webp` (source SHA-256 `63ab14b444c9cfd8527f9fc80440d7fb2507856b745c1d83b10e52e61df0b5f9`). Original graph nodes, colors, and connections remain intact. Urban history, Genetics and evolution, and Human–AI collaboration research are illustrative region names.

The localization sources are retained in the `readme-locales-final` production package. Panorama exports are 6000 × 3600; graph exports are 3600 × 2488; update-history exports are 3600 × 2080. Each was inspected at 900px reading width. The Traditional Chinese panorama and history remain byte-identical to their approved versions; its graph changes only the native English legend and footer text to Traditional Chinese. Longer translations wrap inside the existing composition.

Composition sources: `cover-polish-title` (6000px `cover-polish-hd` master; approved `cover-polish-v2` layout), `real-graph-polish-v7`, `ai-work-film-case-v7` / `ai-work-film-case-v7-localized`, and `lifecycle-polish-v26`, retained in their corresponding production packages, including `cover-polish-title`, `cover-polish-hd`, `readme-final-consistency`, and `ai-work-film-case-v7-localized`. The final title pass reduces only the article heading from 28.16px to 26px; every pixel outside that heading area is identical to the 6000px master. The cover package includes read-only snapshots of the actual editor and hover-preview DOM/styles, original pseudo-element icon CSS, the approved composition, export instructions, and SHA-256 verification. Text and graph labels are rendered at high resolution rather than enlarged from the former small article crop. Heading font fallback is normalized to match the approved capture. Directory/header raster inputs remain unchanged; their detail is limited by the original captures. Browser capture uses JPEG transport; the published PNG adds no further lossy encoding. No generative enhancement or invented UI content is used in the product cover. The AI-work comparison remains an authored concept as noted above.

The older `wenlan-lifecycle*` and `wenlan-knowledge-network*` illustrations and their generators remain available for reference; the current READMEs do not use their old mobile variants.

### Current export hashes

- `wenlan-personal-knowledge-zh-Hant.png`: `328e4bf032d5dcf16cad24658d7d12a51d694fdd2b3d00f5b1e44f94b22246e5`
- `wenlan-personal-graph-zh-Hant.png`: `65a4588af858c942060477578ef2340a0ff967bf6bebde67ebf6d8de66aafc97`
- `wenlan-knowledge-to-work-zh-Hant.png`: `cbf00dc906f128b585c22f6bcf3049fdd4243602c036af174dcc57c894ea39ee`
- `wenlan-knowledge-to-work-zh-Hans.png`: `609cfb9d7d85ebbb37c51749f7baa9b1ddb112e6b1db0acf4fa69cb866ecb52c`
- `wenlan-knowledge-to-work.png`: `90035731a53b9c228b488e38bdd551ea1525dd82aec99d2941926d1f1c1281aa`
- `wenlan-knowledge-to-work-es-ES.png`: `826fef30c558a2cde583fdc0de21ca2b81d76148fa202b3013e863e728045f93`
- `wenlan-updates-history-zh-Hant.png`: `d073e66e5a24f7f202ea1691ac977bac29a343f71ede06a4dc2462b0d3dcf251`
- `wenlan-personal-knowledge.png`: `6aa9bd21bb7e5b746ec1c1545570f0f5df9539fb81ffcf4dc1f915295b2de0e9`
- `wenlan-personal-knowledge-zh-Hans.png`: `96736638a3611964a0b63888e41777f98d07f93e8f9db7b39c964df43dc4eeba`
- `wenlan-personal-graph.png`: `a06050ed7a3a5bb3d43ef31e5a7982b9a640ac328167487b3490bb5da70cccf2`
- `wenlan-personal-graph-zh-Hans.png`: `46330eb6094ca49b149bf05acc6d234d93b5ca3b14d9574fe72d5b9c23607696`
- `wenlan-updates-history.png`: `4a1408d19095c9e57ff21c3e3a0509f9e2ec673caa0c32580bccd8e3163e620d`
- `wenlan-updates-history-zh-Hans.png`: `138cb06111ca09442a48c4e0ec2649d34df8440e90b3cdf32a4f1ce7ab4a9a03`

## Architecture overview

`wenlan-architecture.png`, `-zh-Hant.png`, `-zh-Hans.png`, and `-es-ES.png` are fully localized architecture illustrations, exported at 4800 × 3300. Matching SVG files are the editable vector sources. They are diagrams, not App screenshots. The approved Traditional Chinese composition is preserved; longer translated labels use concise wording and local line wrapping.

The diagram separates imported materials, App/CLI and AI-tool access, the local service, and persistent knowledge. It distinguishes local search models, the external AI client’s own model, and optional background enrichment models. Page citations, dependent-page invalidation, review for human-owned pages, and version history are represented at product level, not as a complete database schema or a guarantee that every retrieval uses every channel.

Source references: [setup and model roles](../setup-and-data.md#models), [retrieval and connected knowledge](../technical-foundations.md), [source invalidation](../../crates/wenlan-core/src/db.rs), and [Page ownership and proposed revisions](../../crates/wenlan-core/src/post_write/page_update.rs). Rendered at 3× from SVG and checked at 900px README width. Existing approved product figures are unchanged.
