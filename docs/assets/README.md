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

All four READMEs use the same approved Traditional Chinese figures, with localized alt text and captions. Native demo UI remains in its captured language.

The final consistency pass aligns the three explanatory titles (46px, semibold, shared left edge), uses Page teal for the editorial Page icon, and moves provenance footnotes into readable README captions. It preserves approved body composition and content. The video cover retains its native UI composition; only its separate footnote strip is removed.

| Asset | Export | Provenance |
| --- | --- | --- |
| `wenlan-personal-knowledge-zh-Hant.png` | 3000 × 1720 | Actual App views combined with demo data and enhanced graph layout. Not a native three-column screen. The approved content area is pixel-identical. |
| `wenlan-personal-graph-zh-Hant.png` | 3600 × 2488 | Actual graph capture; the approved graph body is pixel-identical. Region names and boundaries are editorial annotations, not verified source-node identities or detected communities. |
| `wenlan-knowledge-to-work-zh-Hant.png` | 3600 × 2520 | Fictional project, Claude conversations, and resulting websites. Concept illustration, not a measured comparison. Additional Wenlan context complements Claude's native capabilities. |
| `wenlan-updates-history-zh-Hant.png` | 3600 × 2080 | Actual memory, source, page, and revision captures with demo data; connecting lines and editorial labels are illustrative. Shows automatic updates to untouched AI-organized pages and per-page review for user-created or edited pages. |

The graph uses the website's September 6, 2026 capture, `wenlan-live-knowledge-graph-20260906.webp` (source SHA-256 `63ab14b444c9cfd8527f9fc80440d7fb2507856b745c1d83b10e52e61df0b5f9`). Original graph nodes, colors, and connections remain intact. Urban history, Genetics and evolution, and Human–AI collaboration research are illustrative region names.

Composition sources: `real-graph-polish-v7`, `ai-work-polish-v11`, and `lifecycle-polish-v26`, consolidated into the retained `readme-final-consistency` production package. The package contains HTML compositions, unchanged native capture inputs, export instructions, and SHA-256 verification. Native captures are not generatively redrawn; export resolution does not imply additional detail beyond the original captures.

The older `wenlan-lifecycle*` and `wenlan-knowledge-network*` illustrations and their generators remain available for reference; the current READMEs do not use their old mobile variants.

### Current export hashes

- `wenlan-personal-knowledge-zh-Hant.png`: `f349a6e8a84b4b6185ad99111c4a4216af56cf78df5d97c971971518298d310e`
- `wenlan-personal-graph-zh-Hant.png`: `df7de7ad6dd19041f915ab1b562d14fd0ee9c6396d8b63bd2e912197b145843f`
- `wenlan-knowledge-to-work-zh-Hant.png`: `8a9a22d90282b20003b8222ee111cfcc2c2c6f8d89a48e2165624d2dd8f1ddf5`
- `wenlan-updates-history-zh-Hant.png`: `d073e66e5a24f7f202ea1691ac977bac29a343f71ede06a4dc2462b0d3dcf251`
