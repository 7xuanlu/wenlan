#!/usr/bin/env python3
"""Validate the hand-authored Codex plugin vertical slice.

This is intentionally narrower than the Codex plugin validator. It checks the
Wenlan-specific slice contract for the first manual Codex port before any shared
generator exists.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PLUGIN = ROOT / "plugin-codex"
REQUIRED_SKILLS = ("init", "capture", "brief", "pages")
REQUIRED_SKILL_INTERFACE = {
    "brief": {
        "display_name": "Wenlan Brief",
        "short_description": "Load status, relevant memories, and pending Wenlan revisions",
    },
    "capture": {
        "display_name": "Wenlan Capture",
        "short_description": "Save a durable memory to Wenlan from the current conversation",
    },
    "init": {
        "display_name": "Wenlan Init",
        "short_description": "Set up and verify the local Wenlan daemon and MCP bridge",
    },
    "pages": {
        "display_name": "Wenlan Pages",
        "short_description": "List or open distilled Wenlan pages from Codex",
    },
}
CLAUDE_ONLY_TOKENS = (
    "CLAUDE_PLUGIN_ROOT",
    ".claude-plugin",
    "mcp__plugin_wenlan_wenlan__",
)


def fail(message: str) -> None:
    print(f"codex plugin slice validation failed: {message}", file=sys.stderr)
    raise SystemExit(1)


def read_json(path: Path) -> dict:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"missing {path.relative_to(ROOT)}")
    except json.JSONDecodeError as exc:
        fail(f"{path.relative_to(ROOT)} is not valid JSON: {exc}")
    if not isinstance(payload, dict):
        fail(f"{path.relative_to(ROOT)} must contain a JSON object")
    return payload


def assert_file(path: Path) -> None:
    if not path.is_file():
        fail(f"missing {path.relative_to(ROOT)}")


def assert_no_claude_tokens(path: Path) -> None:
    text = path.read_text(encoding="utf-8")
    for token in CLAUDE_ONLY_TOKENS:
        if token in text:
            fail(f"{path.relative_to(ROOT)} contains Claude-only token {token!r}")


def validate_manifest() -> None:
    manifest = read_json(PLUGIN / ".codex-plugin" / "plugin.json")
    if manifest.get("name") != "wenlan":
        fail("plugin manifest name must stay `wenlan`")
    if "hooks" in manifest:
        fail("Codex plugin manifest must not declare hooks")
    if manifest.get("skills") != "./skills/":
        fail("plugin manifest must point skills at ./skills/")
    if manifest.get("mcpServers") != "./.mcp.json":
        fail("plugin manifest must point mcpServers at ./.mcp.json")
    interface = manifest.get("interface")
    if not isinstance(interface, dict):
        fail("plugin manifest must include interface metadata")
    prompts = interface.get("defaultPrompt")
    if not isinstance(prompts, list) or not 1 <= len(prompts) <= 3:
        fail("interface.defaultPrompt must contain 1-3 starter prompts")


def validate_mcp() -> None:
    mcp = read_json(PLUGIN / ".mcp.json")
    servers = mcp.get("mcpServers")
    if not isinstance(servers, dict):
        fail(".mcp.json must contain mcpServers object")
    wenlan = servers.get("wenlan")
    if not isinstance(wenlan, dict):
        fail(".mcp.json must define the wenlan MCP server")
    if wenlan.get("command") != "./bin/wenlan-mcp-runner.sh":
        fail("wenlan MCP server must run ./bin/wenlan-mcp-runner.sh")


def validate_runner() -> None:
    runner = PLUGIN / "bin" / "wenlan-mcp-runner.sh"
    assert_file(runner)
    text = runner.read_text(encoding="utf-8")
    if "--agent-name" not in text or "codex" not in text:
        fail("Codex MCP runner must pass an explicit codex agent name")


def validate_skills() -> None:
    for skill in REQUIRED_SKILLS:
        path = PLUGIN / "skills" / skill / "SKILL.md"
        metadata_path = PLUGIN / "skills" / skill / "agents" / "openai.yaml"
        assert_file(path)
        assert_file(metadata_path)
        assert_no_claude_tokens(path)
        assert_no_claude_tokens(metadata_path)
        text = path.read_text(encoding="utf-8")
        if f"name: {skill}" not in text:
            fail(f"{path.relative_to(ROOT)} frontmatter must name {skill}")
        if "user-invocable: true" not in text:
            fail(f"{path.relative_to(ROOT)} must be marked user-invocable for slash autocomplete")
        if skill != "pages" and "mcp__wenlan__" not in text:
            fail(f"{path.relative_to(ROOT)} must use Codex wenlan MCP tool names")
        metadata = metadata_path.read_text(encoding="utf-8")
        expected = REQUIRED_SKILL_INTERFACE[skill]
        if "interface:" not in metadata:
            fail(f"{metadata_path.relative_to(ROOT)} must declare Codex app interface metadata")
        for key, value in expected.items():
            expected_line = f'  {key}: "{value}"'
            if expected_line not in metadata:
                fail(f"{metadata_path.relative_to(ROOT)} must contain {expected_line}")


def validate_pages_skill() -> None:
    path = PLUGIN / "skills" / "pages" / "SKILL.md"
    text = path.read_text(encoding="utf-8")
    required = [
        "wenlan pages",
        "Never read a page body",
        "Do not use a picker",
        "If several pages match, print the CLI output",
        "If `wenlan` is not found, tell the user to run `/init`",
    ]
    for needle in required:
        if needle not in text:
            fail(f"{path.relative_to(ROOT)} must contain {needle!r}")
    forbidden = [
        "AskUserQuestion",
        "native picker",
        "command sandbox DISABLED",
        "mcp__plugin_wenlan_wenlan__",
    ]
    for needle in forbidden:
        if needle in text:
            fail(f"{path.relative_to(ROOT)} must not contain {needle!r}")


def main() -> None:
    validate_manifest()
    validate_mcp()
    validate_runner()
    validate_skills()
    validate_pages_skill()
    print("Codex plugin slice validation passed")


if __name__ == "__main__":
    main()
