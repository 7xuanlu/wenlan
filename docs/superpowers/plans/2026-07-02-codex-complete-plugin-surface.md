# Codex Complete Plugin Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port every remaining Wenlan Claude plugin skill into `plugin-codex` with Codex-native guardrails and shared contract validation.

**Architecture:** Keep `wenlan-mcp` and Claude plugin behavior unchanged. Add hand-authored Codex skill files plus a Codex-local resolver copy, then make `plugin-contract.json` and the validators define and enforce full Claude/Codex surface parity.

**Tech Stack:** Bash, Python 3 validator scripts, Codex plugin manifest files, Markdown skill files, Rust distribution tests.

---

## File Structure

- Modify: `plugin-contract.json` so every skill is `shared_now` with Codex metadata flags.
- Modify: `scripts/validate-codex-plugin-slice.py` to derive the required Codex skills from `plugin-contract.json`, validate interface metadata, and enforce skill-specific guardrails.
- Modify: `scripts/validate-plugin-contract.py` to enforce Codex metadata and guardrails at the shared contract layer.
- Modify: `scripts/validate-plugin-contract.test.sh` to add negative drift tests for guardrail and resolver parity failures.
- Create: `plugin-codex/bin/resolve-space.sh` as a Codex-local copy of `plugin/bin/resolve-space.sh`.
- Create: `plugin-codex/bin/test/test-resolve-space.sh` and `plugin-codex/bin/test/fixtures/*.toml` for resolver parity coverage.
- Create: `plugin-codex/skills/{recall,handoff,debrief,distill,curate,forget,help}/SKILL.md`.
- Create: `plugin-codex/skills/{recall,handoff,debrief,distill,curate,forget,help}/agents/openai.yaml`.
- Modify: `plugin-codex/skills/{brief,capture}/SKILL.md` to use `plugin-codex/bin/resolve-space.sh` semantics instead of the temporary inline resolver.
- Modify: `README.md` to list the complete Codex command surface.
- Modify: `plugin-codex/.codex-plugin/plugin.json` only through the plugin-creator cachebuster helper.

### Task 1: Contract Red Tests

**Files:**
- Modify: `plugin-contract.json`
- Modify: `scripts/validate-codex-plugin-slice.py`
- Modify: `scripts/validate-plugin-contract.py`
- Modify: `scripts/validate-plugin-contract.test.sh`

- [ ] **Step 1: Make the contract expect the full shared surface**

Change each `claude_only_until_ported` entry for `curate`, `debrief`, `distill`, `forget`, `handoff`, `help`, and `recall` to:

```json
"status": "shared_now",
"codex_user_invocable": true,
"codex_openai_interface": true
```

- [ ] **Step 2: Make the Codex slice validator derive required skills**

Replace the fixed `REQUIRED_SKILLS` tuple with a loader that reads `plugin-contract.json` and returns all `shared_now` skill names.

```python
def shared_codex_skills() -> tuple[str, ...]:
    contract = read_json(ROOT / "plugin-contract.json")
    skills = []
    for item in contract.get("skills", []):
        if item.get("status") == "shared_now":
            skills.append(item["name"])
    return tuple(sorted(skills))
```

- [ ] **Step 3: Add explicit Codex metadata and guardrail checks**

Add `REQUIRED_SKILL_INTERFACE` entries for the seven new Codex skills and check these phrases:

```python
SKILLS_WITHOUT_MCP_REFERENCE = {"help", "pages"}
REQUIRED_GUARDRAILS = {
    "forget": [
        "cannot be undone",
        "delete <id>",
        "Always confirm with the user before calling forget",
    ],
    "distill": [
        "rebuild <page-id>",
        "force=true",
        "user-edited page prose is wiped",
    ],
    "curate": [
        "revision_source_id",
        "Perform no mutation until the user replies",
        "Ambiguous replies do not mutate",
    ],
    "debrief": [
        "Pending-captures preview",
        "MCP captures",
        "Write session log",
    ],
}
```

- [ ] **Step 4: Run validators and confirm the expected red state**

Run:

```bash
python3 scripts/validate-codex-plugin-slice.py
python3 scripts/validate-plugin-contract.py
```

Expected: both fail because the new Codex skill files and metadata do not exist yet.

### Task 2: Resolver Parity

**Files:**
- Create: `plugin-codex/bin/resolve-space.sh`
- Create: `plugin-codex/bin/test/test-resolve-space.sh`
- Create: `plugin-codex/bin/test/fixtures/spaces-basic.toml`
- Create: `plugin-codex/bin/test/fixtures/spaces-malformed.toml`
- Create: `plugin-codex/bin/test/fixtures/spaces-no-default.toml`
- Create: `plugin-codex/bin/test/fixtures/spaces-trailing-whitespace.toml`
- Modify: `scripts/validate-plugin-contract.test.sh`

- [ ] **Step 1: Add resolver parity negative test**

In `scripts/validate-plugin-contract.test.sh`, add an `assert_rejects` case that edits `plugin-codex/bin/resolve-space.sh` and expects validation to fail.

```bash
assert_rejects "codex resolver parity drift" \
    perl -0pi -e 's/cwd-config-default/codex-default/' \
    "$TMPDIR_TEST/root/plugin-codex/bin/resolve-space.sh"
```

- [ ] **Step 2: Run the contract test and confirm the expected red state**

Run:

```bash
bash scripts/validate-plugin-contract.test.sh
```

Expected: fails because `plugin-codex/bin/resolve-space.sh` is missing.

- [ ] **Step 3: Copy resolver and tests**

Create the Codex resolver and fixtures by copying the Claude resolver test assets:

```bash
mkdir -p plugin-codex/bin/test/fixtures
cp plugin/bin/resolve-space.sh plugin-codex/bin/resolve-space.sh
cp plugin/bin/test/test-resolve-space.sh plugin-codex/bin/test/test-resolve-space.sh
cp plugin/bin/test/fixtures/*.toml plugin-codex/bin/test/fixtures/
```

Then adjust the header comment in the Codex test script so the run command is `./plugin-codex/bin/test/test-resolve-space.sh`.

- [ ] **Step 4: Add validator parity check**

In `scripts/validate-plugin-contract.py`, compare `plugin/bin/resolve-space.sh` and `plugin-codex/bin/resolve-space.sh` exactly.

```python
def validate_resolver_parity(root: Path) -> None:
    claude_resolver = read_text(root, root / "plugin" / "bin" / "resolve-space.sh")
    codex_resolver = read_text(root, root / "plugin-codex" / "bin" / "resolve-space.sh")
    if claude_resolver != codex_resolver:
        fail("plugin-codex/bin/resolve-space.sh must match plugin/bin/resolve-space.sh")
```

- [ ] **Step 5: Verify resolver tests**

Run:

```bash
bash plugin/bin/test/test-resolve-space.sh
bash plugin-codex/bin/test/test-resolve-space.sh
```

Expected: both pass.

### Task 3: Codex Skill Files and Metadata

**Files:**
- Create: `plugin-codex/skills/{recall,handoff,debrief,distill,curate,forget,help}/SKILL.md`
- Create: `plugin-codex/skills/{recall,handoff,debrief,distill,curate,forget,help}/agents/openai.yaml`
- Modify: `plugin-codex/skills/{brief,capture}/SKILL.md`

- [ ] **Step 1: Port the new skill files**

For each new Codex skill, start from the corresponding `plugin/skills/<name>/SKILL.md` content, then apply these exact substitutions:

```text
mcp__plugin_wenlan_wenlan__  ->  mcp__wenlan__
CLAUDE_PLUGIN_ROOT/bin/resolve-space.sh  ->  plugin-codex/bin/resolve-space.sh
AskUserQuestion  ->  compact numbered text protocol
```

Add `user-invocable: true` to every new Codex skill frontmatter.

- [ ] **Step 2: Apply Codex-specific behavior requirements**

Ensure these behavior requirements are present in the relevant skill:

```text
recall: omit `space` when resolver output is unscoped.
handoff: include Pending-captures preview, MCP captures, Write session log.
debrief: duplicate the handoff workflow instructions instead of saying to run /handoff.
distill: require `rebuild <page-id>` confirmation for `force=true`; say user-edited page prose is wiped.
curate: use `wenlan --format json curate`; action keys are `revision_source_id`; no mutation until explicit reply; ambiguity does not mutate.
forget: require `delete <id>` confirmation; always confirm before calling forget; state deletion cannot be undone.
help: print a Codex-specific command card and avoid Claude hook wording.
```

- [ ] **Step 3: Add OpenAI interface metadata**

Each new skill gets `agents/openai.yaml`:

```yaml
interface:
  display_name: "Wenlan Recall"
  short_description: "Search Wenlan memories from Codex"
```

Use these exact pairs for the rest:

```text
handoff -> Wenlan Handoff -> End a Codex session with Wenlan captures and session status
debrief -> Wenlan Debrief -> End a Codex session using the brief/debrief naming pair
distill -> Wenlan Distill -> Synthesize or refresh source-backed Wenlan pages
curate -> Wenlan Curate -> Review pending Wenlan captures or revisions from Codex
forget -> Wenlan Forget -> Delete a Wenlan memory by exact id with confirmation
help -> Wenlan Help -> Show the Codex Wenlan command reference
```

- [ ] **Step 4: Update existing inline resolvers**

Change `brief` and `capture` to call `plugin-codex/bin/resolve-space.sh` and omit the `space` parameter when the resolver returns unscoped.

- [ ] **Step 5: Run validators and confirm green**

Run:

```bash
python3 scripts/validate-codex-plugin-slice.py
python3 scripts/validate-plugin-contract.py
```

Expected: both pass.

### Task 4: README and Cachebuster

**Files:**
- Modify: `README.md`
- Modify: `plugin-codex/.codex-plugin/plugin.json`

- [ ] **Step 1: Update README command surface**

Replace the Codex install section command list with:

```text
/init, /brief, /capture, /recall, /distill, /pages, /curate, /forget,
/handoff, /debrief, /help
```

Keep the reinstall note that a new Codex thread is required after reinstall.

- [ ] **Step 2: Update plugin cachebuster**

Run:

```bash
python3 ~/.codex/skills/.system/plugin-creator/scripts/update_plugin_cachebuster.py plugin-codex
```

Expected: `plugin-codex/.codex-plugin/plugin.json` version suffix changes.

### Task 5: Verification and Review

**Files:**
- All changed files

- [ ] **Step 1: Run focused verification**

Run:

```bash
python3 scripts/validate-codex-plugin-slice.py
python3 scripts/validate-plugin-contract.py
bash scripts/validate-plugin-contract.test.sh
bash plugin/bin/test/test-resolve-space.sh
bash plugin-codex/bin/test/test-resolve-space.sh
cargo test -p wenlan-types --test plugin_distribution pages_skill_replaces_read
git diff --check
git diff --cached --check
```

Expected: all pass.

- [ ] **Step 2: Run Codex plugin manifest validation**

Run in a temporary PyYAML environment:

```bash
tmpvenv="$(mktemp -d)"
python3 -m venv "$tmpvenv"
"$tmpvenv/bin/python" -m pip install -q PyYAML
"$tmpvenv/bin/python" ~/.codex/skills/.system/plugin-creator/scripts/validate_plugin.py plugin-codex
rm -rf "$tmpvenv"
```

Expected: validation passes.

- [ ] **Step 3: Request adversarial review**

Use Claude and AGY reviewer CLIs. Ask them to check:

```text
Does the implementation satisfy docs/superpowers/specs/2026-07-01-codex-complete-plugin-surface-design.md?
Does it keep plugin/ Claude behavior unchanged?
Do validators catch the dangerous drifts: MCP prefix, missing metadata, destructive guardrails, curate mutation ambiguity, debrief thin alias, resolver parity?
```

Expected: no blocking findings, or accepted findings patched and reverified.

- [ ] **Step 4: Commit and push**

Run:

```bash
git status --short
git add -f docs/superpowers/plans/2026-07-02-codex-complete-plugin-surface.md
git add plugin-contract.json scripts/validate-codex-plugin-slice.py scripts/validate-plugin-contract.py scripts/validate-plugin-contract.test.sh plugin-codex README.md
git commit -m "fix: complete Codex plugin surface"
git push
```

Expected: commit and push succeed on `codex/wenlan-codex-pages`.

## Self-Review

- Spec coverage: every skill in the port matrix maps to Task 3; contract and validator requirements map to Tasks 1 and 2; README and cachebuster map to Task 4; review and verification map to Task 5.
- Placeholder scan: no task contains open-ended implementation language; every verification command has an expected result.
- Type and name consistency: skill names match `plugin-contract.json`; resolver paths match the spec; guardrail phrases match the spec exactly.
