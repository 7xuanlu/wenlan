#!/usr/bin/env node

const assert = require("node:assert/strict");
const path = require("node:path");
const test = require("node:test");

const {
  checkPngMatchesExpected,
} = require("./generate-readme-visuals.cjs");

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
