#!/usr/bin/env node

const assert = require("node:assert/strict");
const path = require("node:path");
const test = require("node:test");

const {
  checkPngMatchesExpected,
} = require("./generate-readme-visuals.cjs");
const {
  family,
  makeKnowledgeNetwork,
} = require("./readme-product-visuals.cjs");

const ASSET_DIR = path.resolve(__dirname, "..", "docs", "assets");

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

test("Chinese visual font stacks preserve Wenlan's branded Latin faces", () => {
  assert.equal(
    family("zh-Hans", "heading"),
    '"Fraunces", "Songti SC", "STSong", "PingFang SC", Georgia, serif',
  );
  assert.equal(
    family("zh-Hans", "body"),
    '"Instrument Sans", "PingFang SC", "Hiragino Sans GB", -apple-system, BlinkMacSystemFont, sans-serif',
  );
  assert.equal(
    family("zh-Hans", "mono"),
    '"JetBrains Mono", "PingFang SC", "Hiragino Sans GB", ui-monospace, monospace',
  );
  assert.equal(
    family("zh-Hant", "heading"),
    '"Fraunces", "Songti TC", "STSong", "PingFang TC", Georgia, serif',
  );
  assert.equal(
    family("zh-Hant", "body"),
    '"Instrument Sans", "PingFang TC", "Hiragino Sans CNS", -apple-system, BlinkMacSystemFont, sans-serif',
  );
  assert.equal(
    family("zh-Hant", "mono"),
    '"JetBrains Mono", "PingFang TC", "Hiragino Sans CNS", ui-monospace, monospace',
  );
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
