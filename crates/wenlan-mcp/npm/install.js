#!/usr/bin/env node
"use strict";

const { existsSync, mkdirSync, chmodSync } = require("fs");
const { join } = require("path");
const { spawnSync } = require("child_process");
const https = require("https");
const http = require("http");
const os = require("os");
const fs = require("fs");
const path = require("path");

const VERSION = require("./package.json").version;
const REPO = "7xuanlu/wenlan";

// Maps Node platform-arch to the release bundle filename shipped by release.yml.
// Each bundle contains wenlan, wenlan-server, and wenlan-mcp (or .exe on Windows).
const ASSETS = {
  "darwin-arm64": { file: "wenlan-darwin-arm64.tar.gz",  archive: "tar.gz" },
  "linux-arm64":  { file: "wenlan-linux-arm64.tar.gz",   archive: "tar.gz" },
  "linux-x64":    { file: "wenlan-linux-x64.tar.gz",     archive: "tar.gz" },
  "win32-x64":    { file: "wenlan-windows-x64.zip",      archive: "zip"    },
};

const key = `${process.platform}-${process.arch}`;
const asset = ASSETS[key];

if (!asset) {
  console.error(`Unsupported platform: ${key}`);
  console.error(`Supported: ${Object.keys(ASSETS).join(", ")}`);
  process.exit(1);
}

const isWindows = process.platform === "win32";
const binaryName = isWindows ? "wenlan-mcp.exe" : "wenlan-mcp";
const installName = isWindows ? "wenlan-mcp.exe" : "wenlan-mcp";

const binDir = join(__dirname, "bin");
const dest = join(binDir, installName);

mkdirSync(binDir, { recursive: true });

const downloadUrl = `https://github.com/${REPO}/releases/download/v${VERSION}/${asset.file}`;

console.log(`Downloading wenlan-mcp ${VERSION} for ${key}...`);

function download(url, dest, redirects = 0) {
  return new Promise((resolve, reject) => {
    if (redirects > 10) {
      return reject(new Error("Too many redirects"));
    }
    const mod = url.startsWith("https") ? https : http;
    mod.get(url, { headers: { "User-Agent": "wenlan-mcp-npm" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        return download(res.headers.location, dest, redirects + 1).then(resolve, reject);
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`Download failed: HTTP ${res.statusCode}\nURL: ${url}`));
      }
      const file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on("finish", () => {
        file.close();
        resolve();
      });
      file.on("error", reject);
    }).on("error", reject);
  });
}

function extractBinary(archivePath, archiveType, outputDir) {
  if (archiveType === "tar.gz") {
    // tar is available on macOS, all Linux, and Windows 10 1803+ / Server 2019+.
    const result = spawnSync(
      "tar",
      ["-xzf", archivePath, "-C", outputDir, binaryName],
      { stdio: "inherit" }
    );
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`tar exited with status ${result.status}`);
  } else if (archiveType === "zip") {
    // bsdtar ships with Windows 10 1803+ and can read zip files.
    const result = spawnSync(
      "tar",
      ["-xf", archivePath, "-C", outputDir, binaryName],
      { stdio: "inherit" }
    );
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`tar (zip) exited with status ${result.status}`);
  } else {
    throw new Error(`Unknown archive type: ${archiveType}`);
  }
}

async function install() {
  const tmpDir = os.tmpdir();
  const archivePath = path.join(tmpDir, asset.file);

  try {
    await download(downloadUrl, archivePath);
    extractBinary(archivePath, asset.archive, binDir);

    if (!isWindows) {
      chmodSync(dest, 0o755);
    }

    console.log(`wenlan-mcp installed successfully (${key})`);
  } finally {
    // Clean up downloaded archive regardless of success or failure.
    try {
      if (existsSync(archivePath)) {
        fs.unlinkSync(archivePath);
      }
    } catch (_) {
      // Best-effort cleanup; ignore errors.
    }
  }
}

install().catch((err) => {
  console.error(`Failed to install wenlan-mcp: ${err.message}`);
  process.exit(1);
});
