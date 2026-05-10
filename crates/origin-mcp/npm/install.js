#!/usr/bin/env node
"use strict";

const { createWriteStream, chmodSync, mkdirSync } = require("fs");
const { join } = require("path");
const https = require("https");
const http = require("http");

const VERSION = require("./package.json").version;
const REPO = "7xuanlu/origin-mcp";

const PLATFORM_MAP = {
  "darwin-arm64": "origin-mcp-darwin-arm64",
  "darwin-x64": "origin-mcp-darwin-x64",
  "linux-x64": "origin-mcp-linux-x64",
};

const key = `${process.platform}-${process.arch}`;
const binary = PLATFORM_MAP[key];

if (!binary) {
  console.error(`Unsupported platform: ${key}`);
  console.error(`Supported: ${Object.keys(PLATFORM_MAP).join(", ")}`);
  process.exit(1);
}

const url = `https://github.com/${REPO}/releases/download/v${VERSION}/${binary}`;
const binDir = join(__dirname, "bin");
const dest = join(binDir, "origin-mcp");

mkdirSync(binDir, { recursive: true });

console.log(`Downloading origin-mcp for ${key}...`);

function download(url, dest, redirects = 0) {
  return new Promise((resolve, reject) => {
    if (redirects > 5) {
      return reject(new Error("Too many redirects"));
    }
    const mod = url.startsWith("https") ? https : http;
    mod.get(url, { headers: { "User-Agent": "origin-mcp-npm" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        return download(res.headers.location, dest, redirects + 1).then(resolve, reject);
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`Download failed: HTTP ${res.statusCode}\nURL: ${url}`));
      }
      const file = createWriteStream(dest);
      res.pipe(file);
      file.on("finish", () => {
        file.close();
        chmodSync(dest, 0o755);
        resolve();
      });
    }).on("error", reject);
  });
}

download(url, dest)
  .then(() => console.log("origin-mcp installed successfully"))
  .catch((err) => {
    console.error(`Failed to install origin-mcp: ${err.message}`);
    process.exit(1);
  });
