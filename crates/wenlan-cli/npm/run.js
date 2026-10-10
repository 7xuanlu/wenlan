#!/usr/bin/env node
// SPDX-License-Identifier: Apache-2.0

const childProcess = require("child_process");
const fs = require("fs");
const https = require("https");
const os = require("os");
const path = require("path");

const REPO = "7xuanlu/wenlan";
const ASSET = "wenlan-darwin-arm64.tar.gz";
const REQUESTED_TAG =
  process.env.WENLAN_RELEASE_TAG ||
  process.env.WENLAN_TAG ||
  process.env.ORIGIN_RELEASE_TAG ||
  process.env.ORIGIN_TAG ||
  "";
const BINARIES = ["wenlan", "wenlan-server", "wenlan-mcp"];
// Written into the bin dir only after a COMPLETE install. A run reuses the
// installed binaries when this stamp names the wanted tag and every binary
// still has the size it was extracted with, instead of re-downloading ~60 MB
// on every `npx wenlan` call.
const STAMP_NAME = ".wenlan-install.json";

const RELEASE_TAG_PATTERN = /^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

function safeTag(tag) {
  return tag.replace(/[^A-Za-z0-9._-]/g, "_");
}

function checkTag(tag) {
  if (!RELEASE_TAG_PATTERN.test(tag)) {
    throw new Error(
      `'${tag}' is not a Wenlan release tag (expected vX.Y.Z, like v0.17.0). ` +
        `See https://github.com/${REPO}/releases.`
    );
  }
  return tag;
}

function binDir() {
  if (REQUESTED_TAG) {
    return path.join(os.homedir(), ".wenlan", "releases", safeTag(REQUESTED_TAG));
  }
  return path.join(os.homedir(), ".wenlan", "bin");
}

// The public releases page redirects to the latest tag. Unlike the GitHub API
// it has no limit of 60 anonymous calls per hour per IP address, which a
// shared network can exhaust before a first install.
function latestTag() {
  const url = `https://github.com/${REPO}/releases/latest`;
  const tagPage = `https://github.com/${REPO}/releases/tag/`;
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { "User-Agent": "wenlan" } }, (res) => {
        res.resume();
        try {
          const redirected = res.statusCode >= 300 && res.statusCode < 400;
          const location = (res.headers.location || "").split(/[?#]/)[0];
          if (!redirected || !location.startsWith(tagPage)) {
            throw new Error(
              `Could not find the latest Wenlan release at ${url} (HTTP ${res.statusCode}). ` +
                "Check the network, or pin one with WENLAN_RELEASE_TAG=vX.Y.Z."
            );
          }
          resolve(checkTag(decodeURIComponent(location.slice(tagPage.length))));
        } catch (err) {
          reject(err);
        }
      })
      .on("error", reject);
  });
}

function request(url, headers = {}) {
  return new Promise((resolve, reject) => {
    https
      .get(url, { headers: { "User-Agent": "wenlan", ...headers } }, (res) => {
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

async function download(url, dest) {
  const res = await request(url);
  await new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    res.pipe(file);
    file.on("finish", () => file.close(resolve));
    file.on("error", reject);
  });
}

function extractBinaries(archivePath, dir) {
  const result = childProcess.spawnSync(
    "tar",
    ["-xzf", archivePath, "-C", dir, ...BINARIES],
    { stdio: "inherit" }
  );
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`tar exited with status ${result.status}`);
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
    `\nWenlan binaries are installed in ${dir}.\n` +
      `Add them to your shell PATH if you want to run wenlan directly:\n\n` +
      `  export PATH="${dir}:$PATH"\n\n`
  );
}

function stampPath(dir) {
  return path.join(dir, STAMP_NAME);
}

function readStamp(dir) {
  try {
    const stamp = JSON.parse(fs.readFileSync(stampPath(dir), "utf8"));
    return stamp && typeof stamp === "object" ? stamp : null;
  } catch (_) {
    return null;
  }
}

// null when the install in `dir` is a complete `tag` install that can be
// reused as-is; otherwise the reason it cannot (for tests and diagnostics).
function cacheProblem(dir, tag) {
  const stamp = readStamp(dir);
  if (!stamp) return "no install stamp";
  if (stamp.tag !== tag) return `installed ${stamp.tag}, wanted ${tag}`;
  if (stamp.asset !== ASSET) return `installed asset ${stamp.asset}, wanted ${ASSET}`;
  for (const name of BINARIES) {
    const expected = stamp.sizes && stamp.sizes[name];
    let stat;
    try {
      stat = fs.statSync(path.join(dir, name));
    } catch (_) {
      return `${name} is missing`;
    }
    if (!stat.isFile()) return `${name} is not a file`;
    if (!Number.isInteger(expected) || expected <= 0 || stat.size !== expected) {
      return `${name} is not the size it was installed with`;
    }
    if (process.platform !== "win32" && (stat.mode & 0o111) === 0) {
      return `${name} is not executable`;
    }
  }
  return null;
}

function writeStamp(dir, tag) {
  const sizes = {};
  for (const name of BINARIES) {
    sizes[name] = fs.statSync(path.join(dir, name)).size;
  }
  const finalPath = stampPath(dir);
  const tmpPath = `${finalPath}.${process.pid}.tmp`;
  fs.writeFileSync(tmpPath, JSON.stringify({ tag, asset: ASSET, sizes }) + "\n");
  fs.renameSync(tmpPath, finalPath);
}

function removeStamp(dir) {
  try {
    fs.unlinkSync(stampPath(dir));
  } catch (_) {
    // Already absent.
  }
}

// Every effect is injectable so the cache logic is testable without the
// network, a real release, or the real home directory.
async function installBinaries(overrides = {}) {
  const opts = {
    requestedTag: REQUESTED_TAG,
    dir: binDir(),
    platform: process.platform,
    arch: process.arch,
    archiveDir: os.tmpdir(),
    fetchLatestTag: latestTag,
    downloadFile: download,
    extract: extractBinaries,
    log: (message) => process.stderr.write(message),
    ...overrides,
  };

  if (opts.platform !== "darwin" || opts.arch !== "arm64") {
    throw new Error("Wenlan setup currently supports macOS Apple Silicon only.");
  }

  const dir = opts.dir;
  let tag;
  if (opts.requestedTag) {
    tag = checkTag(opts.requestedTag);
  } else {
    try {
      tag = await opts.fetchLatestTag();
    } catch (err) {
      // Offline (or GitHub unreachable) with a complete install already on
      // disk: run what is installed rather than failing a command that needs
      // no network.
      const installed = readStamp(dir);
      if (installed && installed.tag && cacheProblem(dir, installed.tag) === null) {
        opts.log(
          `Could not check for a newer Wenlan release (${err.message}); ` +
            `using the installed ${installed.tag}.\n`
        );
        return dir;
      }
      throw err;
    }
  }

  if (cacheProblem(dir, tag) === null) {
    return dir;
  }

  fs.mkdirSync(dir, { recursive: true });
  // No stamp may survive into a half-written install: if extraction below is
  // interrupted, the next run must see "no install" and start over.
  removeStamp(dir);

  const archivePath = path.join(opts.archiveDir, ASSET);
  const url = `https://github.com/${REPO}/releases/download/${tag}/${ASSET}`;
  opts.log(`Downloading Wenlan ${tag}...\n`);

  try {
    try {
      await opts.downloadFile(url, archivePath);
    } catch (err) {
      throw new Error(
        `${err.message}. Check that the release exists at https://github.com/${REPO}/releases/tag/${tag}`
      );
    }
    opts.extract(archivePath, dir);
  } finally {
    try {
      if (fs.existsSync(archivePath)) fs.unlinkSync(archivePath);
    } catch (_) {
      // Best-effort cleanup only.
    }
  }

  for (const name of BINARIES) {
    const dest = path.join(dir, name);
    fs.chmodSync(dest, 0o755);
  }

  writeStamp(dir, tag);
  return dir;
}

async function main() {
  const args = process.argv.slice(2);
  const dir = await installBinaries();
  const wenlan = path.join(dir, "wenlan");

  if (args[0] === "setup") {
    const setupArgs = args.slice(1);
    run(wenlan, ["setup", ...(setupArgs.length ? setupArgs : ["--basic"])]);
    run(wenlan, ["background", "on"]);
    run(wenlan, ["status", "--format", "table"]);
    printPathHint(dir);
    return;
  }

  run(wenlan, args.length ? args : ["--help"]);
}

if (require.main === module) {
  main().catch((err) => {
    console.error(`wenlan setup failed: ${err.message}`);
    process.exit(1);
  });
}

module.exports = {
  ASSET,
  BINARIES,
  STAMP_NAME,
  cacheProblem,
  checkTag,
  extractBinaries,
  installBinaries,
  readStamp,
  safeTag,
};
