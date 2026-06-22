#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

const childProcess = require("child_process");
const fs = require("fs");
const https = require("https");
const os = require("os");
const path = require("path");

const REPO = "7xuanlu/origin";
const TARGET = "aarch64-apple-darwin";
const REQUESTED_TAG = process.env.ORIGIN_RELEASE_TAG || process.env.ORIGIN_TAG || "";
const BINARIES = ["origin", "origin-server", "origin-mcp"];

function safeTag(tag) {
  return tag.replace(/[^A-Za-z0-9._-]/g, "_");
}

function binDir() {
  if (REQUESTED_TAG) {
    return path.join(os.homedir(), ".origin", "releases", safeTag(REQUESTED_TAG));
  }
  return path.join(os.homedir(), ".origin", "bin");
}

function githubApiUrl() {
  if (REQUESTED_TAG) {
    return `https://api.github.com/repos/${REPO}/releases/tags/${REQUESTED_TAG}`;
  }
  return `https://api.github.com/repos/${REPO}/releases/latest`;
}

function request(url, headers = {}) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { "User-Agent": "@7xuanlu/origin", ...headers } }, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          res.resume();
          resolve(request(res.headers.location, headers));
          return;
        }
        if (res.statusCode < 200 || res.statusCode >= 300) {
          res.resume();
          reject(new Error(`HTTP ${res.statusCode} for ${url}`));
          return;
        }
        resolve(res);
      })
      .on("error", reject);
  });
}

async function fetchJson(url) {
  const res = await request(url, { Accept: "application/vnd.github+json" });
  let body = "";
  for await (const chunk of res) {
    body += chunk;
  }
  return JSON.parse(body);
}

async function download(url, dest) {
  const res = await request(url);
  await new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest, { mode: 0o755 });
    res.pipe(file);
    file.on("finish", () => file.close(resolve));
    file.on("error", reject);
  });
}

function run(command, args) {
  const result = childProcess.spawnSync(command, args, { stdio: "inherit" });
  if (result.error) throw result.error;
  process.exitCode = result.status || 0;
  if (process.exitCode !== 0) {
    process.exit(process.exitCode);
  }
}

function printPathHint(dir) {
  const entries = (process.env.PATH || "").split(path.delimiter);
  if (entries.includes(dir)) return;

  process.stderr.write(
    `\nOrigin binaries are installed in ${dir}.\n` +
      `Add them to your shell PATH if you want to run origin directly:\n\n` +
      `  export PATH="${dir}:$PATH"\n\n`
  );
}

async function installBinaries() {
  if (process.platform !== "darwin" || process.arch !== "arm64") {
    throw new Error("Origin setup currently supports macOS Apple Silicon only.");
  }

  const release = await fetchJson(githubApiUrl());
  const tag = release.tag_name;
  if (!tag) throw new Error("Could not determine Origin release tag.");

  const dir = binDir();
  fs.mkdirSync(dir, { recursive: true });

  for (const name of BINARIES) {
    const dest = path.join(dir, name);
    const url = `https://github.com/${REPO}/releases/download/${tag}/${name}-${TARGET}`;
    process.stderr.write(`Downloading ${name} ${tag}...\n`);
    await download(url, dest);
    fs.chmodSync(dest, 0o755);
  }

  return dir;
}

async function main() {
  const args = process.argv.slice(2);
  const dir = await installBinaries();
  const origin = path.join(dir, "origin");

  if (args[0] === "setup") {
    const setupArgs = args.slice(1);
    run(origin, ["setup", ...(setupArgs.length ? setupArgs : ["--basic"])]);
    run(origin, ["install"]);
    run(origin, ["status", "--format", "table"]);
    printPathHint(dir);
    return;
  }

  run(origin, args.length ? args : ["--help"]);
}

main().catch((err) => {
  console.error(`origin setup failed: ${err.message}`);
  process.exit(1);
});
