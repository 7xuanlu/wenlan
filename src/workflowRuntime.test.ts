// SPDX-License-Identifier: AGPL-3.0-only
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const workflows = [
  ".github/workflows/backend-pin-bump.yml",
  ".github/workflows/ci.yml",
  ".github/workflows/release.yml",
] as const;

describe("GitHub Actions runtime floor", () => {
  it.each(workflows)("%s avoids Node 20 action releases", (path) => {
    const workflow = readFileSync(resolve(path), "utf8");

    expect(workflow).not.toMatch(
      /(?:actions\/checkout|actions\/setup-node|pnpm\/action-setup)@v4/,
    );
  });

  it.each([
    ".github/workflows/ci.yml",
    ".github/workflows/release.yml",
  ])("%s runs project scripts on Node 24", (path) => {
    const workflow = readFileSync(resolve(path), "utf8");

    expect(workflow).toContain("node-version: 24");
    expect(workflow).not.toContain("node-version: 20");
  });
});
