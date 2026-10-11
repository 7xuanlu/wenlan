// SPDX-License-Identifier: AGPL-3.0-only
// Run with: node --test scripts/gen-codex-plugin.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { generateSkill } from "./gen-codex-plugin.mjs";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const script = join(repo, "scripts/gen-codex-plugin.mjs");
const INPUTS = ["plugin", "plugin-codex", "plugin-contract.json"];

function tempTree() {
  const root = mkdtempSync(join(tmpdir(), "gen-codex-plugin-"));
  for (const f of INPUTS) cpSync(join(repo, f), join(root, f), { recursive: true });
  return root;
}

const run = (root, ...flags) =>
  spawnSync(process.execPath, [script, "--root", root, ...flags], { encoding: "utf8" });

const withTree = (fn) => {
  const root = tempTree();
  try {
    fn(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
};

const contract = JSON.parse(readFileSync(join(repo, "plugin-contract.json"), "utf8"));
const overlay = JSON.parse(readFileSync(join(repo, "plugin-codex/overlay.json"), "utf8"));
const claudeSkill = (name) => readFileSync(join(repo, `plugin/skills/${name}/SKILL.md`), "utf8");

// A minimal Claude skill for unit-level transform checks.
const SAMPLE = `---
name: sample
description: >
  Sample skill. Invoked as \`/sample\`.
argument-hint: "<x>"
allowed-tools: ["mcp__plugin_wenlan_wenlan__recall", "AskUserQuestion", "Bash"]
---

# /sample

Intro for Claude Code.

\`\`\`bash
raw_args="<args>"
resolved="$("$CLAUDE_PLUGIN_ROOT/scripts/resolve-space.sh" --cwd "$PWD" --args "$raw_args")"
PLUGIN_JSON="\${CLAUDE_PLUGIN_ROOT:-plugin}/.claude-plugin/plugin.json"
"\${CLAUDE_PLUGIN_ROOT:-plugin}/scripts/version-drift.sh" --manifest "$PLUGIN_JSON"
\`\`\`

## Keep

Call \`mcp__plugin_wenlan_wenlan__recall\` with "caller_id": "claude-code".

## Drop me

\`\`\`text
## not a heading, inside a fence
\`\`\`

Gone on Codex.

## Replace me

Old line one.
Old line two.
`;
const SAMPLE_OPENAI = { display_name: "Wenlan Sample", short_description: "Sample" };

test("real tree: every generated file is in sync", () => {
  const res = run(repo, "--check");
  assert.equal(res.status, 0, res.stderr);
});

test("generation is idempotent and --check agrees", () => {
  withTree((root) => {
    const first = run(root);
    assert.equal(first.status, 0, first.stderr);
    assert.equal(first.stdout, "", "a clean copy of the repo must need no rewrite");
    assert.equal(run(root, "--check").status, 0);
  });
});

test("--check fails on a hand edit to a generated skill and names the file", () => {
  withTree((root) => {
    const file = join(root, "plugin-codex/skills/brief/SKILL.md");
    writeFileSync(file, readFileSync(file, "utf8").replace("# /brief", "# /brief (edited)"));
    const res = run(root, "--check");
    assert.equal(res.status, 1);
    assert.match(res.stderr, /plugin-codex\/skills\/brief\/SKILL\.md is out of date/);
    assert.match(res.stderr, /run: node scripts\/gen-codex-plugin\.mjs/);
  });
});

test("without --check, a hand edit is overwritten from the Claude skill", () => {
  withTree((root) => {
    const file = join(root, "plugin-codex/skills/brief/SKILL.md");
    const expected = readFileSync(file, "utf8");
    writeFileSync(file, "garbage");
    const res = run(root);
    assert.equal(res.status, 0, res.stderr);
    assert.match(res.stdout, /updated plugin-codex\/skills\/brief\/SKILL\.md/);
    assert.equal(readFileSync(file, "utf8"), expected);
  });
});

test("an overlay `from` run that no longer matches the Claude skill is a hard error", () => {
  withTree((root) => {
    const file = join(root, "plugin/skills/setup/SKILL.md");
    writeFileSync(
      file,
      readFileSync(file, "utf8").replace(
        "- Right after `/plugin install wenlan@7xuanlu-wenlan`.",
        "- Right after installing.",
      ),
    );
    const res = run(root, "--check");
    assert.equal(res.status, 2);
    assert.match(res.stderr, /setup: replace\[\d+\] matched 0 runs/);
  });
});

test("a Claude-only token the overlay did not remove fails generation", () => {
  withTree((root) => {
    const file = join(root, "plugin/skills/forget/SKILL.md");
    writeFileSync(file, readFileSync(file, "utf8").replace("## Call", "## Call (never via AskUserQuestion)"));
    const res = run(root, "--check");
    assert.equal(res.status, 2);
    assert.match(res.stderr, /forget: generated SKILL\.md still contains "AskUserQuestion"/);
  });
});

test("every shared skill in the contract has an overlay entry and vice versa", () => {
  const shared = contract.skills.filter((s) => s.status === "shared_now").map((s) => s.name).sort();
  assert.deepEqual(Object.keys(overlay.skills).sort(), shared);
  const dirs = readdirSync(join(repo, "plugin-codex/skills"), { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort();
  assert.deepEqual(dirs, shared, "plugin-codex/skills must hold exactly the generated skills");
});

test("a bare --root is a usage error, not a crash", () => {
  const res = spawnSync(process.execPath, [script, "--root"], { encoding: "utf8" });
  assert.equal(res.status, 64);
  assert.match(res.stderr, /usage:/);
});

test("frontmatter: Bash first, AskUserQuestion dropped, user-invocable after allowed-tools, prefix swapped", () => {
  const { skill } = generateSkill("sample", SAMPLE, { openai: SAMPLE_OPENAI }, contract);
  const lines = skill.split("\n");
  assert.equal(lines[0], "---");
  assert.equal(lines[1], "name: sample");
  const tools = lines.findIndex((l) => l.startsWith("allowed-tools:"));
  assert.equal(lines[tools], 'allowed-tools: ["Bash", "mcp__wenlan__recall"]');
  assert.equal(lines[tools + 1], "user-invocable: true");
  assert.equal(lines[tools + 2], "---");
});

test("frontmatter: a skill without Bash does not gain it", () => {
  const src = SAMPLE.replace(
    'allowed-tools: ["mcp__plugin_wenlan_wenlan__recall", "AskUserQuestion", "Bash"]',
    'allowed-tools: ["mcp__plugin_wenlan_wenlan__list_pending_imports"]',
  ).replace(/resolved=.*\n/, "");
  const { skill } = generateSkill("sample", src, { openai: SAMPLE_OPENAI }, contract);
  assert.match(skill, /\nallowed-tools: \["mcp__wenlan__list_pending_imports"\]\nuser-invocable: true\n---/);
});

test("description override is emitted as a folded block", () => {
  const { skill } = generateSkill(
    "sample",
    SAMPLE,
    { openai: SAMPLE_OPENAI, description: ["Line one.", "Line two."] },
    contract,
  );
  assert.match(skill, /\ndescription: >\n  Line one\.\n  Line two\.\nargument-hint:/);
});

test("body: resolver line becomes the Codex locator block, plugin paths and host words swap", () => {
  const { skill } = generateSkill("sample", SAMPLE, { openai: SAMPLE_OPENAI }, contract);
  assert.match(skill, /# The resolver ships inside the installed plugin; the relative path only\n# exists in a wenlan checkout\.\n/);
  assert.match(
    skill,
    /resolver="\$\(find "\$HOME\/\.codex\/plugins\/cache" -path '\*\/wenlan\/\*\/bin\/resolve-space\.sh' 2>\/dev\/null \| head -1\)"\n/,
  );
  assert.match(
    skill,
    /resolved="\$\("\$\{resolver:-plugin-codex\/bin\/resolve-space\.sh\}" --cwd "\$PWD" --args "\$raw_args"\)"\n/,
  );
  assert.match(skill, /PLUGIN_JSON="\$\{CODEX_PLUGIN_ROOT:-plugin-codex\}\/\.codex-plugin\/plugin\.json"/);
  assert.match(skill, /"\$\{CODEX_PLUGIN_ROOT:-plugin-codex\}\/bin\/version-drift\.sh"/);
  assert.match(skill, /Intro for Codex\./);
  assert.match(skill, /"caller_id": "codex"/);
  assert.match(skill, /Call `mcp__wenlan__recall`/);
  for (const token of ["CLAUDE_PLUGIN_ROOT", ".claude-plugin", "mcp__plugin_wenlan_wenlan__", "AskUserQuestion", "Claude Code"]) {
    assert.ok(!skill.includes(token), `leftover ${token}`);
  }
});

test("sections: null drops through the next heading, fenced '##' lines are not headings", () => {
  const { skill } = generateSkill(
    "sample",
    SAMPLE,
    { openai: SAMPLE_OPENAI, sections: { "## Drop me": null } },
    contract,
  );
  assert.ok(!skill.includes("## Drop me"));
  assert.ok(!skill.includes("Gone on Codex."));
  assert.ok(!skill.includes("## not a heading"));
  assert.ok(skill.includes("## Replace me\n\nOld line one."), "the following section survives intact");
});

test("sections: an array replaces heading and body; an unknown heading fails", () => {
  const { skill } = generateSkill(
    "sample",
    SAMPLE,
    { openai: SAMPLE_OPENAI, sections: { "## Keep": ["## Kept", "", "New body.", ""] } },
    contract,
  );
  assert.match(skill, /## Kept\n\nNew body\.\n\n## Drop me/);
  assert.throws(
    () => generateSkill("sample", SAMPLE, { openai: SAMPLE_OPENAI, sections: { "## Nope": null } }, contract),
    /section "## Nope" matched 0 headings/,
  );
});

test("replace: exact run swaps, ambiguous or missing runs fail", () => {
  const { skill } = generateSkill(
    "sample",
    SAMPLE,
    { openai: SAMPLE_OPENAI, replace: [{ from: ["Old line one.", "Old line two."], to: ["New line."] }] },
    contract,
  );
  assert.match(skill, /## Replace me\n\nNew line\.\n$/);
  assert.throws(
    () => generateSkill("sample", SAMPLE, { openai: SAMPLE_OPENAI, replace: [{ from: ["Old line two.", "Old line one."], to: [] }] }, contract),
    /replace\[0\] matched 0 runs/,
  );
  const doubled = SAMPLE + "\nOld line one.\n";
  assert.throws(
    () => generateSkill("sample", doubled, { openai: SAMPLE_OPENAI, replace: [{ from: ["Old line one."], to: null }] }, contract),
    /replace\[0\] matched 2 runs/,
  );
});

test("openai.yaml is rendered from the overlay and the real files match the slice validator pins", () => {
  const { openai } = generateSkill("sample", SAMPLE, { openai: SAMPLE_OPENAI }, contract);
  assert.equal(openai, 'interface:\n  display_name: "Wenlan Sample"\n  short_description: "Sample"\n');
  assert.throws(
    () => generateSkill("sample", SAMPLE, { openai: { display_name: "x" } }, contract),
    /overlay openai needs display_name and short_description/,
  );
});

test("real tree: generated skills carry the pins the validators and Rust tests read", () => {
  const codex = (name) => readFileSync(join(repo, `plugin-codex/skills/${name}/SKILL.md`), "utf8");
  assert.match(codex("setup"), /curl -fsSL https:\/\/raw\.githubusercontent\.com\/7xuanlu\/wenlan\/v\$\{RELEASE_VER\}\/install\.sh \| bash/);
  assert.ok(!codex("setup").includes("npx -y wenlan@"), "Codex setup must not inherit the npm installer");
  assert.ok(claudeSkill("setup").includes("npx -y wenlan@"), "Claude setup keeps the npm installer");
  for (const needle of ["Do not use a picker", "If several pages match, print the CLI output", "Never read a page body"]) {
    assert.ok(codex("pages").includes(needle), `pages: ${needle}`);
  }
  for (const needle of ["Perform no mutation until the user replies", "Ambiguous replies do not mutate", "revision_source_id"]) {
    assert.ok(codex("curate").includes(needle), `curate: ${needle}`);
  }
  assert.ok(codex("handoff").includes('"caller_id": "codex"'));
  // The version pin the release bump rewrites lives in three places that must agree.
  const pin = claudeSkill("setup").match(/npx -y wenlan@(\d+\.\d+\.\d+) setup/)[1];
  assert.match(codex("setup"), new RegExp(`/v${pin.replaceAll(".", "\\.")}/install\\.sh`));
  assert.match(JSON.stringify(overlay), new RegExp(`npx -y wenlan@${pin.replaceAll(".", "\\.")} setup`));
});
