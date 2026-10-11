// SPDX-License-Identifier: AGPL-3.0-only
// Generates every Codex skill (plugin-codex/skills/<skill>/SKILL.md and
// agents/openai.yaml) from the Claude skill of the same name plus
// plugin-codex/overlay.json. The Claude file is the only hand-written prose;
// the overlay carries the structural differences (sections or line runs that
// Codex replaces or drops, the per-skill `agents/openai.yaml` metadata, and a
// description override). `--check` exits 1 when a generated file differs;
// `--root <dir>` points at another tree.
//
// Pipeline per skill, in order:
//   1. overlay `sections`  : heading -> replacement lines, or null to drop
//   2. overlay `replace`   : exact contiguous line runs -> replacement, or null
//   3. MCP tool prefix     : claude prefix -> codex prefix (from plugin-contract.json)
//   4. frontmatter         : drop AskUserQuestion, Bash first, user-invocable: true,
//                            optional `description` override
//   5. resolver line       : "$CLAUDE_PLUGIN_ROOT/scripts/resolve-space.sh" -> the
//                            Codex locator block
//   6. plugin paths        : ${CLAUDE_PLUGIN_ROOT:-plugin}/.claude-plugin/plugin.json and
//                            /scripts/version-drift.sh -> Codex paths
//   7. host words          : "caller_id": "claude-code" -> "codex"; Claude Code -> Codex
//   8. leftover guard      : fail on CLAUDE_PLUGIN_ROOT, .claude-plugin, the Claude
//                            MCP prefix, AskUserQuestion
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const OVERLAY_PATH = "plugin-codex/overlay.json";
const CONTRACT_PATH = "plugin-contract.json";

const CODEX_RESOLVER_BLOCK = [
  "# The resolver ships inside the installed plugin; the relative path only",
  "# exists in a wenlan checkout.",
  "resolver=\"$(find \"$HOME/.codex/plugins/cache\" -path '*/wenlan/*/bin/resolve-space.sh' 2>/dev/null | head -1)\"",
];
const CLAUDE_RESOLVER_RE =
  /^resolved="\$\("\$CLAUDE_PLUGIN_ROOT\/scripts\/resolve-space\.sh"(.*)\)"$/;

// Any of these in a generated skill means the overlay or a swap rule missed a
// Claude-only detail. `/wenlan:` is the Claude namespaced slash form (Codex
// skills are bare `/setup`); `npx -y wenlan@` is the Claude installer.
const LEFTOVER_TOKENS = [
  "CLAUDE_PLUGIN_ROOT",
  ".claude-plugin",
  "AskUserQuestion",
  "claude-code",
  "/wenlan:",
  "npx -y wenlan@",
];

class GenError extends Error {}

function fail(skill, message) {
  throw new GenError(`${skill}: ${message}`);
}

// --- markdown helpers -------------------------------------------------------

function splitFrontmatter(skill, text) {
  const lines = text.split("\n");
  if (lines[0] !== "---") fail(skill, "SKILL.md must start with frontmatter");
  const end = lines.indexOf("---", 1);
  if (end < 0) fail(skill, "unterminated frontmatter");
  return { frontmatter: lines.slice(1, end), body: lines.slice(end + 1) };
}

// Index of every markdown heading line that is not inside a fenced code block.
function headingIndexes(lines) {
  const out = [];
  let fence = null;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const open = line.match(/^(`{3,}|~{3,})/);
    if (fence) {
      if (open && open[1][0] === fence[0] && open[1].length >= fence.length) fence = null;
      continue;
    }
    if (open) {
      fence = open[1];
      continue;
    }
    if (/^#{1,6} /.test(line)) out.push(i);
  }
  return out;
}

// A section runs from its heading line to the line before the next heading of
// any level (flat, no nesting). `null` drops it; an array replaces it.
function applySections(skill, body, sections) {
  let lines = body;
  for (const [heading, replacement] of Object.entries(sections)) {
    const heads = headingIndexes(lines);
    const matches = heads.filter((i) => lines[i] === heading);
    if (matches.length !== 1) {
      fail(skill, `section ${JSON.stringify(heading)} matched ${matches.length} headings, need exactly 1`);
    }
    const start = matches[0];
    const next = heads.find((i) => i > start);
    const end = next === undefined ? lines.length : next;
    if (replacement !== null && !Array.isArray(replacement)) {
      fail(skill, `section ${JSON.stringify(heading)} must map to an array of lines or null`);
    }
    lines = [...lines.slice(0, start), ...(replacement ?? []), ...lines.slice(end)];
  }
  return lines;
}

function findRun(lines, run) {
  const hits = [];
  outer: for (let i = 0; i + run.length <= lines.length; i++) {
    for (let j = 0; j < run.length; j++) {
      if (lines[i + j] !== run[j]) continue outer;
    }
    hits.push(i);
  }
  return hits;
}

// Each entry: { from: [lines], to: [lines] | null }. `from` must match exactly
// one contiguous run in the Claude body (after `sections`).
function applyReplace(skill, body, replace) {
  let lines = body;
  replace.forEach((entry, n) => {
    const { from, to } = entry;
    if (!Array.isArray(from) || from.length === 0) fail(skill, `replace[${n}].from must be a non-empty array`);
    if (to !== null && !Array.isArray(to)) fail(skill, `replace[${n}].to must be an array or null`);
    const hits = findRun(lines, from);
    if (hits.length !== 1) {
      fail(skill, `replace[${n}] matched ${hits.length} runs (first line ${JSON.stringify(from[0])}), need exactly 1`);
    }
    lines = [...lines.slice(0, hits[0]), ...(to ?? []), ...lines.slice(hits[0] + from.length)];
  });
  return lines;
}

// --- frontmatter -----------------------------------------------------------

function parseFrontmatter(skill, lines) {
  // Ordered list of { key, value: string[] } (value lines without the "key:" head).
  const fields = [];
  for (const line of lines) {
    const m = line.match(/^([a-z-]+):(.*)$/);
    if (m) {
      fields.push({ key: m[1], value: [m[2].replace(/^ /, "")] });
    } else if (fields.length && /^\s/.test(line)) {
      fields.at(-1).value.push(line);
    } else {
      fail(skill, `unparseable frontmatter line ${JSON.stringify(line)}`);
    }
  }
  return fields;
}

function renderFrontmatter(fields) {
  const out = [];
  for (const { key, value } of fields) {
    out.push(value[0] === "" ? `${key}:` : `${key}: ${value[0]}`);
    out.push(...value.slice(1));
  }
  return out;
}

function transformFrontmatter(skill, lines, overlay) {
  const fields = parseFrontmatter(skill, lines);
  const byKey = (k) => fields.find((f) => f.key === k);
  if (byKey("name")?.value[0] !== skill) fail(skill, "frontmatter name mismatch");
  if (byKey("user-invocable")) fail(skill, "Claude skill must not set user-invocable; the generator adds it");

  if (overlay.description !== undefined) {
    const d = byKey("description");
    if (!d) fail(skill, "description override needs a description field");
    if (!Array.isArray(overlay.description) || overlay.description.length === 0) {
      fail(skill, "overlay description must be a non-empty array of lines");
    }
    d.value = [">", ...overlay.description.map((l) => `  ${l}`)];
  }

  const tools = byKey("allowed-tools");
  if (tools) {
    let parsed;
    try {
      parsed = JSON.parse(tools.value.join("\n"));
    } catch {
      fail(skill, "allowed-tools must be a JSON array");
    }
    if (!Array.isArray(parsed)) fail(skill, "allowed-tools must be a JSON array");
    const rest = parsed.filter((t) => t !== "AskUserQuestion" && t !== "Bash");
    const codex = parsed.includes("Bash") ? ["Bash", ...rest] : rest;
    tools.value = [`[${codex.map((t) => JSON.stringify(t)).join(", ")}]`];
    fields.splice(fields.indexOf(tools) + 1, 0, { key: "user-invocable", value: ["true"] });
  } else {
    fields.push({ key: "user-invocable", value: ["true"] });
  }
  return renderFrontmatter(fields);
}

// --- body transforms -------------------------------------------------------

function swapResolver(skill, lines, codexRoot) {
  const out = [];
  for (const line of lines) {
    const m = line.match(CLAUDE_RESOLVER_RE);
    if (!m) {
      out.push(line);
      continue;
    }
    out.push(
      ...CODEX_RESOLVER_BLOCK,
      `resolved="$("\${resolver:-${codexRoot}/bin/resolve-space.sh}"${m[1]})"`,
    );
  }
  return out;
}

function swapPluginPaths(text, claudeRoot, codexRoot) {
  return text
    .replaceAll(
      `\${CLAUDE_PLUGIN_ROOT:-${claudeRoot}}/.claude-plugin/plugin.json`,
      `\${CODEX_PLUGIN_ROOT:-${codexRoot}}/.codex-plugin/plugin.json`,
    )
    .replaceAll(
      `\${CLAUDE_PLUGIN_ROOT:-${claudeRoot}}/scripts/version-drift.sh`,
      `\${CODEX_PLUGIN_ROOT:-${codexRoot}}/bin/version-drift.sh`,
    );
}

function swapHostWords(text) {
  return text
    .replaceAll('"caller_id": "claude-code"', '"caller_id": "codex"')
    .replaceAll("Claude Code", "Codex")
    // Claude invokes plugin skills namespaced (`/wenlan:setup`); Codex skills
    // are bare (`/setup`), as plugin-codex/README.md documents.
    .replaceAll("/wenlan:", "/");
}

function guardLeftovers(skill, text, claudePrefix, codexPrefix) {
  for (const token of [...LEFTOVER_TOKENS, claudePrefix]) {
    if (text.includes(token)) {
      const line = text.split("\n").findIndex((l) => l.includes(token)) + 1;
      fail(skill, `generated SKILL.md still contains ${JSON.stringify(token)} (line ${line}); add an overlay replace for it`);
    }
  }
  // The prefix swap above is global, so the Claude prefix can never survive it;
  // what can survive is a misspelt prefix. Every MCP tool name must carry the
  // Codex prefix.
  const stray = text.split("\n").findIndex((l) => /mcp__/.test(l.replaceAll(codexPrefix, "")));
  if (stray >= 0) {
    fail(skill, `line ${stray + 1} names an MCP tool without the ${JSON.stringify(codexPrefix)} prefix`);
  }
}

// --- openai.yaml -----------------------------------------------------------

const yamlString = (s) => JSON.stringify(s);

function renderOpenai(skill, openai) {
  if (!openai || typeof openai.display_name !== "string" || typeof openai.short_description !== "string") {
    fail(skill, "overlay openai needs display_name and short_description strings");
  }
  return [
    "interface:",
    `  display_name: ${yamlString(openai.display_name)}`,
    `  short_description: ${yamlString(openai.short_description)}`,
    "",
  ].join("\n");
}

// --- driver ----------------------------------------------------------------

export function generateSkill(skill, claudeText, overlay, contract) {
  const claude = contract.surfaces.claude;
  const codex = contract.surfaces.codex;
  const { frontmatter, body } = splitFrontmatter(skill, claudeText);

  let lines = body;
  if (overlay.sections) lines = applySections(skill, lines, overlay.sections);
  if (overlay.replace) lines = applyReplace(skill, lines, overlay.replace);
  lines = swapResolver(skill, lines, codex.plugin_root);

  const fm = transformFrontmatter(skill, frontmatter, overlay);
  let text = ["---", ...fm, "---", ...lines].join("\n");
  text = text.replaceAll(claude.skill_mcp_tool_prefix, codex.skill_mcp_tool_prefix);
  text = swapPluginPaths(text, claude.plugin_root, codex.plugin_root);
  text = swapHostWords(text);
  guardLeftovers(skill, text, claude.skill_mcp_tool_prefix, codex.skill_mcp_tool_prefix);
  return { skill: text, openai: renderOpenai(skill, overlay.openai) };
}

export function generateAll(root) {
  const contract = JSON.parse(readFileSync(resolve(root, CONTRACT_PATH), "utf8"));
  const overlayAll = JSON.parse(readFileSync(resolve(root, OVERLAY_PATH), "utf8"));
  const skills = contract.skills.filter((s) => s.status === "shared_now").map((s) => s.name);
  const overlaySkills = Object.keys(overlayAll.skills ?? {});
  const unknown = overlaySkills.filter((s) => !skills.includes(s));
  if (unknown.length) throw new GenError(`${OVERLAY_PATH} names skills not shared in ${CONTRACT_PATH}: ${unknown.join(", ")}`);
  const missing = skills.filter((s) => !overlaySkills.includes(s));
  if (missing.length) throw new GenError(`${OVERLAY_PATH} is missing skills: ${missing.join(", ")} (every shared skill needs at least an openai entry)`);

  const claudeRoot = contract.surfaces.claude.plugin_root;
  const codexRoot = contract.surfaces.codex.plugin_root;
  const outputs = [];
  for (const skill of skills) {
    const src = resolve(root, claudeRoot, "skills", skill, "SKILL.md");
    const claudeText = readFileSync(src, "utf8");
    const gen = generateSkill(skill, claudeText, overlayAll.skills[skill], contract);
    outputs.push({ path: `${codexRoot}/skills/${skill}/SKILL.md`, text: gen.skill });
    outputs.push({ path: `${codexRoot}/skills/${skill}/agents/openai.yaml`, text: gen.openai });
  }
  return outputs;
}

function diffHint(path, actual, expected) {
  const a = actual.split("\n");
  const e = expected.split("\n");
  const out = [`${path} is out of date with its Claude skill + ${OVERLAY_PATH}:`];
  for (let i = 0; i < Math.max(a.length, e.length); i++) {
    if (a[i] === e[i]) continue;
    out.push(`  line ${i + 1}:`, `  - ${a[i] ?? "<missing>"}`, `  + ${e[i] ?? "<missing>"}`);
    if (out.length > 13) {
      out.push("  ...");
      break;
    }
  }
  return out.join("\n");
}

function rootValue(argv, idx) {
  const value = argv[idx + 1];
  if (value === undefined || value.startsWith("--")) {
    console.error("usage: gen-codex-plugin.mjs [--check] [--root <dir>]");
    process.exit(64);
  }
  return value;
}

function main() {
  const args = process.argv.slice(2);
  const check = args.includes("--check");
  const rootIdx = args.indexOf("--root");
  const root =
    rootIdx >= 0
      ? resolve(rootValue(args, rootIdx))
      : resolve(dirname(fileURLToPath(import.meta.url)), "..");

  let outputs;
  try {
    outputs = generateAll(root);
  } catch (err) {
    if (err instanceof GenError) {
      console.error(`gen-codex-plugin: ${err.message}`);
      process.exit(2);
    }
    throw err;
  }

  let stale = 0;
  for (const { path, text } of outputs) {
    const file = resolve(root, path);
    const actual = existsSync(file) ? readFileSync(file, "utf8") : "";
    if (actual === text) continue;
    if (check) {
      stale++;
      console.error(diffHint(path, actual, text));
    } else {
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, text);
      console.log(`updated ${path}`);
    }
  }
  if (stale) {
    console.error("run: node scripts/gen-codex-plugin.mjs");
    process.exit(1);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
