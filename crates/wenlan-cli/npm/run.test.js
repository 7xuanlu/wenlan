// SPDX-License-Identifier: Apache-2.0
// Run with: node --test crates/wenlan-cli/npm/run.test.js
//
// Not shipped: package.json's `files` allowlist is run.js, README.md, LICENSE.
// Every effect of installBinaries (network, tar, home directory) is injected,
// so these tests use a temp directory and never touch ~/.wenlan.

const assert = require("node:assert/strict");
const childProcess = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { afterEach, beforeEach, describe, test } = require("node:test");

const {
  ASSET,
  BINARIES,
  cacheProblem,
  extractBinaries,
  installBinaries,
  readStamp,
} = require("./run.js");

let root;
let dir;
let archiveDir;

beforeEach(() => {
  root = fs.mkdtempSync(path.join(os.tmpdir(), "wenlan-run-test-"));
  dir = path.join(root, "bin");
  archiveDir = path.join(root, "tmp");
  fs.mkdirSync(archiveDir);
});

afterEach(() => {
  fs.rmSync(root, { recursive: true, force: true });
});

// Records every effect so a test can assert what did and did not happen.
function harness({ latest = "v1.0.0", requestedTag = "", extractBody } = {}) {
  const calls = { latest: 0, download: [], extract: 0, log: [] };
  return {
    calls,
    opts: {
      requestedTag,
      dir,
      archiveDir,
      platform: "darwin",
      arch: "arm64",
      log: (message) => calls.log.push(message),
      fetchLatestTag: async () => {
        calls.latest += 1;
        if (latest instanceof Error) throw latest;
        return latest;
      },
      downloadFile: async (url, dest) => {
        calls.download.push(url);
        fs.writeFileSync(dest, "archive");
      },
      extract: (_archive, into) => {
        calls.extract += 1;
        if (extractBody) return extractBody(into);
        for (const name of BINARIES) {
          fs.writeFileSync(path.join(into, name), `#!/bin/sh\n# ${name}\n`);
        }
      },
    },
  };
}

describe("installBinaries version cache", () => {
  test("fresh install downloads once, announces it, and stamps the tag", async () => {
    const h = harness();
    const result = await installBinaries(h.opts);

    assert.equal(result, dir);
    assert.deepEqual(h.calls.download, [
      `https://github.com/7xuanlu/wenlan/releases/download/v1.0.0/${ASSET}`,
    ]);
    assert.equal(h.calls.extract, 1);
    assert.deepEqual(h.calls.log, ["Downloading Wenlan v1.0.0...\n"]);
    const stamp = readStamp(dir);
    assert.equal(stamp.tag, "v1.0.0");
    assert.equal(stamp.asset, ASSET);
    assert.deepEqual(Object.keys(stamp.sizes).sort(), [...BINARIES].sort());
    assert.equal(cacheProblem(dir, "v1.0.0"), null);
    for (const name of BINARIES) {
      assert.ok(fs.statSync(path.join(dir, name)).mode & 0o111, `${name} executable`);
    }
  });

  test("second run of the same tag reuses the binaries with no download", async () => {
    await installBinaries(harness().opts);

    const again = harness();
    const result = await installBinaries(again.opts);

    assert.equal(result, dir);
    assert.deepEqual(again.calls.download, []);
    assert.equal(again.calls.extract, 0);
    assert.deepEqual(again.calls.log, [], "a cache hit is silent");
  });

  test("a newer release re-downloads over the stamped older one", async () => {
    await installBinaries(harness({ latest: "v1.0.0" }).opts);

    const upgrade = harness({ latest: "v1.1.0" });
    await installBinaries(upgrade.opts);

    assert.equal(upgrade.calls.download.length, 1);
    assert.match(upgrade.calls.download[0], /\/v1\.1\.0\//);
    assert.equal(readStamp(dir).tag, "v1.1.0");
  });

  test("a truncated binary is not trusted and is re-downloaded", async () => {
    await installBinaries(harness().opts);
    fs.writeFileSync(path.join(dir, "wenlan-server"), "");

    assert.match(cacheProblem(dir, "v1.0.0"), /wenlan-server/);
    const repair = harness();
    await installBinaries(repair.opts);
    assert.equal(repair.calls.download.length, 1);
    assert.equal(cacheProblem(dir, "v1.0.0"), null);
  });

  test("a missing binary is re-downloaded", async () => {
    await installBinaries(harness().opts);
    fs.unlinkSync(path.join(dir, "wenlan-mcp"));

    const repair = harness();
    await installBinaries(repair.opts);
    assert.equal(repair.calls.download.length, 1);
  });

  test("a non-executable binary is re-downloaded", async () => {
    await installBinaries(harness().opts);
    fs.chmodSync(path.join(dir, "wenlan"), 0o644);

    assert.match(cacheProblem(dir, "v1.0.0"), /not executable/);
  });

  test("binaries with no stamp (older runner, other installer) are re-downloaded once", async () => {
    fs.mkdirSync(dir, { recursive: true });
    for (const name of BINARIES) fs.writeFileSync(path.join(dir, name), "old");

    const h = harness();
    await installBinaries(h.opts);
    assert.equal(h.calls.download.length, 1);
    assert.equal(readStamp(dir).tag, "v1.0.0");
  });

  test("an interrupted extraction leaves no stamp, so the next run starts over", async () => {
    await installBinaries(harness().opts);

    const broken = harness({
      latest: "v1.1.0",
      extractBody: (into) => {
        fs.writeFileSync(path.join(into, "wenlan"), "partial");
        throw new Error("tar exited with status 1");
      },
    });
    await assert.rejects(installBinaries(broken.opts), /tar exited/);
    assert.equal(readStamp(dir), null, "stale v1.0.0 stamp must not vouch for the partial v1.1.0 files");

    const next = harness({ latest: "v1.1.0" });
    await installBinaries(next.opts);
    assert.equal(next.calls.download.length, 1);
    assert.equal(readStamp(dir).tag, "v1.1.0");
  });

  test("a pinned tag never asks GitHub for the latest release", async () => {
    const pinned = harness({ requestedTag: "v0.9.0", latest: new Error("must not be called") });
    await installBinaries(pinned.opts);
    assert.equal(pinned.calls.latest, 0);
    assert.match(pinned.calls.download[0], /\/v0\.9\.0\//);

    const again = harness({ requestedTag: "v0.9.0", latest: new Error("must not be called") });
    await installBinaries(again.opts);
    assert.equal(again.calls.latest, 0);
    assert.deepEqual(again.calls.download, []);
  });

  test("offline with a complete install runs the installed version and says so", async () => {
    await installBinaries(harness({ latest: "v1.0.0" }).opts);

    const offline = harness({ latest: new Error("getaddrinfo ENOTFOUND github.com") });
    const result = await installBinaries(offline.opts);

    assert.equal(result, dir);
    assert.deepEqual(offline.calls.download, []);
    assert.equal(offline.calls.log.length, 1);
    assert.match(offline.calls.log[0], /ENOTFOUND/);
    assert.match(offline.calls.log[0], /v1\.0\.0/);
  });

  test("offline with nothing installed still fails with the network error", async () => {
    const offline = harness({ latest: new Error("getaddrinfo ENOTFOUND github.com") });
    await assert.rejects(installBinaries(offline.opts), /ENOTFOUND/);
    assert.deepEqual(offline.calls.download, []);
  });

  test("offline with a damaged install does not pretend it is usable", async () => {
    await installBinaries(harness().opts);
    fs.writeFileSync(path.join(dir, "wenlan"), "");

    const offline = harness({ latest: new Error("getaddrinfo ENOTFOUND github.com") });
    await assert.rejects(installBinaries(offline.opts), /ENOTFOUND/);
  });

  test("a failed download keeps its explanation and leaves no stamp", async () => {
    const h = harness();
    h.opts.downloadFile = async () => {
      throw new Error("HTTP 404 for https://example.invalid");
    };
    await assert.rejects(installBinaries(h.opts), /HTTP 404.*Check that the release exists/);
    assert.equal(readStamp(dir), null);
  });

  test("anything but macOS Apple Silicon is refused before touching disk", async () => {
    const h = harness();
    h.opts.platform = "linux";
    await assert.rejects(installBinaries(h.opts), /macOS Apple Silicon only/);
    assert.equal(fs.existsSync(dir), false);
    assert.equal(h.calls.latest, 0);
  });
});

describe("extractBinaries", () => {
  const tar = childProcess.spawnSync("tar", ["--version"]);

  test(
    "extracts exactly the three binaries from a release-shaped archive",
    { skip: tar.error ? "tar is not installed" : false },
    () => {
      const staging = path.join(root, "staging");
      fs.mkdirSync(staging);
      for (const name of BINARIES) fs.writeFileSync(path.join(staging, name), `bin ${name}`);
      fs.writeFileSync(path.join(staging, "README.txt"), "not extracted");
      const archive = path.join(root, ASSET);
      const made = childProcess.spawnSync("tar", ["-czf", archive, "-C", staging, ...BINARIES, "README.txt"]);
      assert.equal(made.status, 0);

      fs.mkdirSync(dir);
      extractBinaries(archive, dir);

      assert.deepEqual(fs.readdirSync(dir).sort(), [...BINARIES].sort());
      assert.equal(fs.readFileSync(path.join(dir, "wenlan"), "utf8"), "bin wenlan");
    }
  );
});
