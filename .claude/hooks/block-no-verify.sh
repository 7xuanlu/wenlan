#!/usr/bin/env bash
# PreToolUse hook on Bash: block git commands using --no-verify or --no-gpg-sign.
# Enforces CLAUDE.md rule: "Never skip hooks (--no-verify) or bypass signing".
set -euo pipefail

INPUT="$(cat)"
CMD="$(printf '%s' "$INPUT" | /usr/bin/python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("command",""))' 2>/dev/null || true)"

# Only gate actual git invocations — otherwise an innocent grep/heredoc that merely
# mentions "--no-verify" gets blocked.
case "$CMD" in
  *git\ *)
    case "$CMD" in
      *"--no-verify"*|*"--no-gpg-sign"*|*"-c commit.gpgsign=false"*)
        {
          echo "🛑 Blocked: git command uses --no-verify / --no-gpg-sign / signing-bypass."
          echo "    Command: $CMD"
          echo "    Per project policy hooks must run. Fix the underlying issue."
        } >&2
        exit 2
        ;;
    esac
    ;;
esac

exit 0
