#!/usr/bin/env node

const assert = require("node:assert/strict");
const path = require("node:path");
const test = require("node:test");

const {
  checkPngMatchesExpected,
} = require("./generate-readme-visuals.cjs");
const {
  family,
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
