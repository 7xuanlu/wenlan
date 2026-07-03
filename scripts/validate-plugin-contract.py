#!/usr/bin/env python3
"""Validate the shared Wenlan Claude/Codex plugin contract.

The contract is deliberately small: it documents which skills are shared now,
which Claude skills are not ported yet, and the surface-specific MCP, runner,
and marketplace rules that should not drift silently.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


DEFAULT_ROOT = Path(__file__).resolve().parents[1]
ALLOWED_SKILL_STATUSES = {"shared_now", "claude_only_until_ported"}
CODEX_SKILLS_WITHOUT_MCP_REFERENCE = {"help", "pages"}
CODEX_SKILLS_USING_RESOLVER = {"brief", "capture", "distill", "handoff", "recall"}
CODEX_REQUIRED_GUARDRAILS = {
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
}


def fail(message: str) -> None:
    print(f"plugin contract validation failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def rel(root: Path, path: Path) -> str:
    try:
        return str(path.relative_to(root))
    except ValueError:
        return str(path)


def read_json(root: Path, path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"missing {rel(root, path)}")
    except json.JSONDecodeError as exc:
        fail(f"{rel(root, path)} is not valid JSON: {exc}")
    if not isinstance(payload, dict):
        fail(f"{rel(root, path)} must contain a JSON object")
    return payload


def read_text(root: Path, path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        fail(f"missing {rel(root, path)}")


def require_file(root: Path, path: Path) -> None:
    if not path.is_file():
        fail(f"missing {rel(root, path)}")


def require_equal(label: str, actual: Any, expected: Any) -> None:
    if actual != expected:
        fail(f"{label} must be {expected!r}, got {actual!r}")


def load_contract(root: Path) -> dict[str, Any]:
    contract = read_json(root, root / "plugin-contract.json")
    require_equal("plugin-contract.json schema_version", contract.get("schema_version"), 1)
    surfaces = contract.get("surfaces")
    if not isinstance(surfaces, dict):
        fail("plugin-contract.json must contain surfaces object")
    for surface in ("claude", "codex"):
        if not isinstance(surfaces.get(surface), dict):
            fail(f"plugin-contract.json surfaces must include {surface}")
    skills = contract.get("skills")
    if not isinstance(skills, list) or not skills:
        fail("plugin-contract.json must contain non-empty skills array")
    return contract


def contract_skill_sets(contract: dict[str, Any]) -> tuple[set[str], set[str]]:
    names: set[str] = set()
    shared_now: set[str] = set()
    claude_only: set[str] = set()
    for item in contract["skills"]:
        if not isinstance(item, dict):
            fail("each plugin-contract skill entry must be an object")
        name = item.get("name")
        status = item.get("status")
        if not isinstance(name, str) or not name:
            fail("each plugin-contract skill entry must have a name")
        if name in names:
            fail(f"duplicate plugin-contract skill entry {name!r}")
        names.add(name)
        if status not in ALLOWED_SKILL_STATUSES:
            fail(f"skill {name} has unsupported status {status!r}")
        if status == "shared_now":
            if item.get("codex_user_invocable") is not True:
                fail(f"shared skill {name} must set codex_user_invocable true")
            if item.get("codex_openai_interface") is not True:
                fail(f"shared skill {name} must set codex_openai_interface true")
            shared_now.add(name)
        elif status == "claude_only_until_ported":
            claude_only.add(name)
            if not item.get("reason"):
                fail(f"claude-only skill {name} must document why it is not ported yet")
    return shared_now, claude_only


def skill_names(root: Path, plugin_root: Path) -> set[str]:
    skills_root = plugin_root / "skills"
    if not skills_root.is_dir():
        fail(f"missing {rel(root, skills_root)}")
    return {
        path.parent.name
        for path in skills_root.glob("*/SKILL.md")
        if path.is_file()
    }


def parse_frontmatter(root: Path, path: Path) -> dict[str, str]:
    text = read_text(root, path)
    lines = text.splitlines()
    if not lines or lines[0] != "---":
        fail(f"{rel(root, path)} must start with frontmatter")
    metadata: dict[str, str] = {}
    for line in lines[1:]:
        if line == "---":
            return metadata
        match = re.match(r"^([A-Za-z0-9_-]+):\s*(.*)$", line)
        if match:
            metadata[match.group(1)] = match.group(2).strip().strip('"')
    fail(f"{rel(root, path)} frontmatter is not closed")


def validate_manifest(root: Path, surface: str, config: dict[str, Any]) -> None:
    manifest_path = root / config["manifest_path"]
    manifest = read_json(root, manifest_path)
    require_equal(f"{rel(root, manifest_path)} name", manifest.get("name"), config["manifest_name"])
    if surface == "codex":
        require_equal(
            f"{rel(root, manifest_path)} skills",
            manifest.get("skills"),
            config["manifest_skills"],
        )
        require_equal(
            f"{rel(root, manifest_path)} mcpServers",
            manifest.get("mcpServers"),
            config["manifest_mcp_servers"],
        )
        if "hooks" in manifest:
            fail(f"{rel(root, manifest_path)} must not declare hooks")


def validate_mcp_config(root: Path, surface: str, config: dict[str, Any]) -> None:
    path = root / config["mcp_config_path"]
    mcp = read_json(root, path)
    servers = mcp.get("mcpServers")
    if not isinstance(servers, dict):
        fail(f"{rel(root, path)} must contain mcpServers object")
    server_name = config["mcp_server_name"]
    server = servers.get(server_name)
    if not isinstance(server, dict):
        fail(f"{rel(root, path)} must define MCP server {server_name!r}")
    require_equal(
        f"{surface} MCP command",
        server.get("command"),
        config["mcp_command"],
    )


def validate_runner(root: Path, surface: str, config: dict[str, Any]) -> None:
    runner_path = root / config["runner_path"]
    runner = read_text(root, runner_path)
    agent = config["agent_name"]
    if agent["mode"] == "runner_argument_default":
        env = agent["env"]
        value = agent["value"]
        if f'{env}:-{value}' not in runner:
            fail(f"{rel(root, runner_path)} must default {env} to {value}")
        if '--agent-name "${agent_name}"' not in runner:
            fail(f"{rel(root, runner_path)} must pass --agent-name through the runner")
    elif agent["mode"] == "wenlan_mcp_stdio_default":
        if "--agent-name" in runner:
            fail(f"{rel(root, runner_path)} must rely on the wenlan-mcp stdio default agent")
        source = read_text(root, root / "crates" / "wenlan-mcp" / "src" / "main.rs")
        pattern = re.compile(
            r"if\s+serve_args\.is_some\(\)\s*\{.*?\"remote-mcp\"\.into\(\).*?\}"
            r"\s*else\s*\{.*?\"" + re.escape(agent["value"]) + r"\"\.into\(\).*?\}",
            re.DOTALL,
        )
        if not pattern.search(source):
            fail(f"wenlan-mcp stdio default agent must remain {agent['value']!r}")
    else:
        fail(f"{surface} has unsupported agent mode {agent['mode']!r}")


def validate_resolver_parity(root: Path) -> None:
    claude_resolver = read_text(root, root / "plugin" / "bin" / "resolve-space.sh")
    codex_resolver = read_text(root, root / "plugin-codex" / "bin" / "resolve-space.sh")
    if claude_resolver != codex_resolver:
        fail("plugin-codex/bin/resolve-space.sh must match plugin/bin/resolve-space.sh")


def iter_matching_plugin(plugins: list[Any], plugin_name: str):
    for item in plugins:
        if isinstance(item, dict) and item.get("name") == plugin_name:
            yield item


def validate_marketplace(root: Path, config: dict[str, Any]) -> None:
    marketplace_config = config.get("marketplace")
    if not isinstance(marketplace_config, dict):
        fail("codex contract must include marketplace object")
    path = root / marketplace_config["path"]
    marketplace = read_json(root, path)
    require_equal(f"{rel(root, path)} name", marketplace.get("name"), marketplace_config["name"])
    plugins = marketplace.get("plugins")
    if not isinstance(plugins, list):
        fail(f"{rel(root, path)} must contain plugins array")
    plugin = next(iter_matching_plugin(plugins, marketplace_config["plugin_name"]), None)
    if plugin is None:
        fail(f"{rel(root, path)} must contain plugin {marketplace_config['plugin_name']!r}")
    source = plugin.get("source")
    if not isinstance(source, dict):
        fail(f"{rel(root, path)} plugin source must be an object")
    require_equal("Codex marketplace source", source.get("source"), marketplace_config["source"])
    require_equal(
        "Codex marketplace source.path",
        source.get("path"),
        marketplace_config["source_path"],
    )
    policy = plugin.get("policy")
    if not isinstance(policy, dict):
        fail(f"{rel(root, path)} plugin policy must be an object")
    require_equal(
        "Codex marketplace policy.installation",
        policy.get("installation"),
        marketplace_config["policy_installation"],
    )
    require_equal(
        "Codex marketplace policy.authentication",
        policy.get("authentication"),
        marketplace_config["policy_authentication"],
    )
    require_equal(
        "Codex marketplace category",
        plugin.get("category"),
        marketplace_config["category"],
    )


def validate_skill_surface(
    root: Path,
    surface: str,
    config: dict[str, Any],
    expected_names: set[str],
    shared_now: set[str],
    contract: dict[str, Any],
) -> None:
    plugin_root = root / config["plugin_root"]
    actual_names = skill_names(root, plugin_root)
    if actual_names != expected_names:
        fail(
            f"{surface} skill inventory drift: expected {sorted(expected_names)}, "
            f"got {sorted(actual_names)}"
        )

    expected_prefix = config["skill_mcp_tool_prefix"]
    other_prefix = (
        contract["surfaces"]["codex"]["skill_mcp_tool_prefix"]
        if surface == "claude"
        else contract["surfaces"]["claude"]["skill_mcp_tool_prefix"]
    )

    for name in sorted(expected_names):
        skill_path = plugin_root / "skills" / name / "SKILL.md"
        require_file(root, skill_path)
        frontmatter = parse_frontmatter(root, skill_path)
        require_equal(f"{rel(root, skill_path)} frontmatter name", frontmatter.get("name"), name)
        text = read_text(root, skill_path)
        if other_prefix in text:
            fail(f"{rel(root, skill_path)} contains wrong MCP tool prefix {other_prefix!r}")
        mcp_tools = set(re.findall(r"mcp__[A-Za-z0-9_]+__[A-Za-z0-9_]+", text))
        wrong_tools = sorted(token for token in mcp_tools if not token.startswith(expected_prefix))
        if wrong_tools:
            fail(f"{rel(root, skill_path)} contains unexpected MCP tools: {wrong_tools}")
        if mcp_tools and not any(token.startswith(expected_prefix) for token in mcp_tools):
            fail(f"{rel(root, skill_path)} must use MCP prefix {expected_prefix!r}")

        if surface == "codex" and name in shared_now:
            require_equal(
                f"{rel(root, skill_path)} user-invocable",
                frontmatter.get("user-invocable"),
                "true",
            )
            if name not in CODEX_SKILLS_WITHOUT_MCP_REFERENCE and expected_prefix not in text:
                fail(f"{rel(root, skill_path)} must use MCP prefix {expected_prefix!r}")
            if name in CODEX_SKILLS_USING_RESOLVER and "plugin-codex/bin/resolve-space.sh" not in text:
                fail(f"{rel(root, skill_path)} must use plugin-codex/bin/resolve-space.sh")
            for needle in CODEX_REQUIRED_GUARDRAILS.get(name, []):
                if needle not in text:
                    fail(f"{rel(root, skill_path)} must contain guardrail {needle!r}")
            metadata_path = plugin_root / "skills" / name / "agents" / "openai.yaml"
            require_file(root, metadata_path)
            metadata = read_text(root, metadata_path)
            if "interface:" not in metadata:
                fail(f"{rel(root, metadata_path)} must declare interface metadata")


def validate_contract(root: Path) -> None:
    contract = load_contract(root)
    shared_now, claude_only = contract_skill_sets(contract)
    surfaces = contract["surfaces"]

    for surface, config in surfaces.items():
        validate_manifest(root, surface, config)
        validate_mcp_config(root, surface, config)
        validate_runner(root, surface, config)

    validate_resolver_parity(root)
    validate_marketplace(root, surfaces["codex"])

    validate_skill_surface(
        root,
        "claude",
        surfaces["claude"],
        shared_now | claude_only,
        shared_now,
        contract,
    )
    validate_skill_surface(
        root,
        "codex",
        surfaces["codex"],
        shared_now,
        shared_now,
        contract,
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    args = parser.parse_args()
    validate_contract(args.root.resolve())
    print("Plugin contract validation passed")


if __name__ == "__main__":
    main()
