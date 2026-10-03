#!/usr/bin/env node

const assert = require("node:assert/strict");
const path = require("node:path");
const test = require("node:test");

const {
  checkPngMatchesExpected,
  makeBanner,
} = require("./generate-readme-visuals.cjs");
const {
  family,
  makeOverview,
  makeLifecycle,
  makeKnowledgeNetwork,
} = require("./readme-product-visuals.cjs");

const ASSET_DIR = path.resolve(__dirname, "..", "docs", "assets");

test("approved banners use the simple four-locale tagline and preserve HEAD geometry", () => {
  const expected = {
    en: ["A living personal wiki.", "AI organizes. You stay in control."],
    "zh-Hant": ["持續更新的個人維基。", "AI 幫你整理，你保有主導權。"],
    "zh-Hans": ["持续更新的个人维基。", "AI 帮你整理，你保有主导权。"],
    "es-ES": ["Una wiki personal viva.", "La IA organiza. Tú tienes el control."],
  };

  for (const [locale, lines] of Object.entries(expected)) {
    for (const viewport of ["desktop", "mobile"]) {
      const banner = makeBanner(viewport, locale);
      const suffix = locale === "en" ? "" : `-${locale}`;
      assert.equal(banner.name, `readme-banner${suffix}${viewport === "mobile" ? "-mobile" : ""}`);
      assert.deepEqual([banner.width, banner.height], viewport === "desktop" ? [1280, 440] : [720, 300]);
      assert.match(banner.svg, new RegExp(`>${lines[0].replace(/[.*+?^${}()|[\]\\]/gu, "\\$&")}<`, "u"));
      assert.match(banner.svg, new RegExp(`>${lines[1].replace(/[.*+?^${}()|[\]\\]/gu, "\\$&")}<`, "u"));
      assert.equal((banner.svg.match(/letter-spacing="4"/gu) ?? []).length, 1);
      assert.doesNotMatch(banner.svg, /data-fit-region|input-1|input-2|topic-page|\[1\]|\[2\]/u);
      assert.match(
        banner.svg,
        viewport === "desktop"
          ? /transform="translate\(144 104\) scale\(0\.453125\)"/u
          : /transform="translate\(294 30\) scale\(0\.2578125\)"/u,
      );
      assert.match(
        banner.svg,
        viewport === "desktop"
          ? /<rect x="64" y="28" width="1152" height="384" rx="30"/u
          : /<rect x="24" y="16" width="672" height="268" rx="24"/u,
      );
    }
  }
});

test("PNG verification rejects different content with identical dimensions", async () => {
  const expectedPath = path.join(ASSET_DIR, "wenlan-system.png");
  const differentPath = path.join(ASSET_DIR, "wenlan-system-zh-Hans.png");

  const errors = await checkPngMatchesExpected(differentPath, expectedPath);

  assert.equal(errors.length, 1);
  assert.match(errors[0], /does not match generated output/u);
});

test("PNG verification accepts byte-identical generated output", async () => {
  const expectedPath = path.join(ASSET_DIR, "wenlan-system.png");

  const errors = await checkPngMatchesExpected(expectedPath, expectedPath);

  assert.deepEqual(errors, []);
});

test("PNG verification reports dimension mismatches", async () => {
  const currentPath = path.join(ASSET_DIR, "wenlan-system-mobile.png");
  const expectedPath = path.join(ASSET_DIR, "wenlan-system.png");

  const errors = await checkPngMatchesExpected(currentPath, expectedPath);

  assert.equal(errors.length, 1);
  assert.match(errors[0], /is \d+x\d+; generated output is \d+x\d+/u);
});

test("all diagram locales reuse the app's typography tokens", () => {
  const css = require("node:fs").readFileSync(path.join(__dirname, "../src/index.css"), "utf8");
  for (const locale of ["en", "zh-Hans", "zh-Hant"]) {
    for (const kind of ["heading", "body", "mono"]) {
      const expected = css.match(new RegExp(`--mem-font-${kind}:\\s*([^;]+);`))[1];
      assert.equal(family(locale, kind), expected);
      assert.doesNotMatch(family(locale, kind), /Songti|STSong/u);
    }
  }
});

test("knowledge-network visual distinguishes entities and relation semantics", () => {
  const asset = makeKnowledgeNetwork("en", "desktop");

  assert.equal(asset.name, "wenlan-knowledge-network");
  assert.match(asset.svg, />KNOWLEDGE PAGE</u);
  assert.match(asset.svg, />ENTITY</u);
  assert.doesNotMatch(asset.svg, /ENTITY PAGE/u);
  assert.match(asset.svg, />SOURCE PAGE</u);
  assert.match(asset.svg, />MEMORY</u);
  assert.match(asset.svg, /marker-end="url\(#network-en-desktop-relation-arrow\)"/u);
  assert.match(asset.svg, />PART OF</u);
  assert.match(asset.svg, />RELATED TO · 0\.82</u);
  assert.match(asset.svg, />GROUPED BY RELATION DENSITY</u);
  assert.doesNotMatch(asset.svg, /IN PROGRESS/u);
});

test("knowledge-network visual has desktop and mobile assets for both Chinese locales", () => {
  assert.equal(
    makeKnowledgeNetwork("zh-Hans", "desktop").name,
    "wenlan-knowledge-network-zh-Hans",
  );
  assert.equal(
    makeKnowledgeNetwork("zh-Hant", "mobile").name,
    "wenlan-knowledge-network-zh-Hant-mobile",
  );
});

test("every knowledge-network locale keeps direction and strength semantics", () => {
  for (const locale of ["en", "zh-Hans", "zh-Hant"]) {
    for (const viewport of ["desktop", "mobile"]) {
      const { svg } = makeKnowledgeNetwork(locale, viewport);
      const graphBody = svg.split("<defs>")[0];
      const sagePaths = graphBody.match(/<path[^>]+stroke="#6F8F76"[^>]+\/>/gu) ?? [];
      const directedPaths = graphBody.match(/<path[^>]+marker-end=[^>]+\/>/gu) ?? [];

      assert.equal(sagePaths.length, 5, `${locale}/${viewport} sage relation count`);
      assert.equal(directedPaths.length, 5, `${locale}/${viewport} directed relation count`);
      for (const pathMarkup of sagePaths) {
        assert.match(pathMarkup, /marker-end=/u, `${locale}/${viewport} sage relation direction`);
      }
      assert.equal((svg.match(/0\.82/gu) ?? []).length, 1, `${locale}/${viewport} confidence exemplar`);
      assert.doesNotMatch(svg, /ENTITY PAGE|实体页面|實體頁面/u);
    }
  }
});

test("diagrams retain bounded model, review, and citation claims", () => {
  for (const viewport of ["desktop", "mobile"]) {
    const overview = makeOverview("en", viewport).svg;
    const lifecycle = makeLifecycle("en", viewport).svg;
    const network = makeKnowledgeNetwork("en", viewport).svg;
    assert.match(overview, /configured model/u);
    assert.match(overview, /Automatic refresh proposes changes/u);
    assert.match(lifecycle, /CITATION CHECK PASSED/u);
    assert.match(lifecycle, /AUTOMATIC REFRESH · EDITED PAGE/u);
    assert.match(lifecycle, /Supersession keeps the earlier memory/u);
    assert.doesNotMatch(lifecycle, /VERIFIED REBUILD|Archive, never delete|On-device \/ off/u);
    assert.match(network, /Conceptual example/u);
    assert.doesNotMatch(network, /Pages connect through Entities/u);
  }
});
