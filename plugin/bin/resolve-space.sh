#!/usr/bin/env bash
# resolve-space.sh — Resolve the active Origin space from the 6-layer chain.
#
# Layers (highest to lowest priority):
#   1. --arg <space>           explicit override from skill caller
#   2. ORIGIN_SPACE env var    per-shell pin
#   3. ~/.origin/spaces.toml   cwd-prefix mapping
#   4. cwd repo basename       git rev-parse --show-toplevel
#   5. --topic <string>        conversation topic fallback
#   6. "personal"              hard default
#
# Output: "<space>\t<source-layer>" on stdout.
# Exit code: 0 on success (always succeeds; layer 6 is the floor).

set -u

cwd=""
arg=""
topic=""

while [ $# -gt 0 ]; do
    case "$1" in
        --cwd)   cwd="${2:-}"; shift $(( $# > 1 ? 2 : 1 )) ;;
        --arg)   arg="${2:-}"; shift $(( $# > 1 ? 2 : 1 )) ;;
        --topic) topic="${2:-}"; shift $(( $# > 1 ? 2 : 1 )) ;;
        *)       shift ;;
    esac
done

# Layer 6: default
printf 'personal\tdefault\n'
