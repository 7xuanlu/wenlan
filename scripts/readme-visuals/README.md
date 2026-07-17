# README visual toolchain

Install the locked renderer and its matching Chromium build:

```bash
npm ci --ignore-scripts --prefix scripts/readme-visuals
npm run --prefix scripts/readme-visuals install-browser
```

Generate or verify every README visual:

```bash
npm run --prefix scripts/readme-visuals generate
npm run --prefix scripts/readme-visuals check
```

The checked-in PNG files are the canonical README render. SVG files preserve
editable text and geometry, so standalone SVG typography can vary when the
named fonts are unavailable.

Byte-for-byte checks currently require macOS because the Chinese assets use
PingFang and Songti. The locale stacks mirror the Wenlan app's typography:
Fraunces for headings, Instrument Sans for body copy, and JetBrains Mono for
labels, followed by native CJK fallbacks per glyph. Keeping the branded Latin
faces first is important for mixed strings such as `WENLAN`, `Markdown`, and
record IDs. The Latin render fonts are bundled in `../readme-visual-fonts/`.
