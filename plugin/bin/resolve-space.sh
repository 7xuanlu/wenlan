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

# Layer 2: ORIGIN_SPACE env var (per-shell pin)
if [ -n "${ORIGIN_SPACE:-}" ]; then
    printf '%s\tenv\n' "$ORIGIN_SPACE"
    exit 0
fi

# Layer 3: cwd-config TOML mapping
# Reads file at $SPACES_FILE (override), else ~/.origin/spaces.toml.
# Parses [[mapping]] blocks; longest prefix matching $cwd wins.
spaces_file="${SPACES_FILE:-$HOME/.origin/spaces.toml}"
if [ -n "$cwd" ] && [ -f "$spaces_file" ]; then
    best_prefix=""
    best_space=""
    # Walk file; collect (prefix,space) pairs from [[mapping]] blocks.
    # Bash 3.2 compat: no associative arrays. Use parallel files in a tmp dir.
    tmp_pairs="$(mktemp)"
    awk '
        BEGIN { in_block = 0; cur_prefix = ""; cur_space = "" }
        /^\[\[mapping\]\]/ {
            if (in_block && cur_prefix != "" && cur_space != "") {
                print cur_prefix "\t" cur_space
            }
            in_block = 1; cur_prefix = ""; cur_space = ""; next
        }
        in_block && /^prefix[ \t]*=/ {
            sub(/^prefix[ \t]*=[ \t]*/, "")
            gsub(/^"|"$/, "")
            cur_prefix = $0
        }
        in_block && /^space[ \t]*=/ {
            sub(/^space[ \t]*=[ \t]*/, "")
            gsub(/^"|"$/, "")
            cur_space = $0
        }
        END {
            if (in_block && cur_prefix != "" && cur_space != "") {
                print cur_prefix "\t" cur_space
            }
        }
    ' "$spaces_file" 2>/dev/null > "$tmp_pairs"

    # Expand ~ to $HOME in prefixes; find longest match against cwd.
    while IFS=$'\t' read -r prefix space; do
        case "$prefix" in
            "~/"*) expanded="${HOME}/${prefix#~/}" ;;
            "~")   expanded="$HOME" ;;
            *)     expanded="$prefix" ;;
        esac
        case "$cwd" in
            "$expanded"|"$expanded/"*)
                if [ ${#expanded} -gt ${#best_prefix} ]; then
                    best_prefix="$expanded"
                    best_space="$space"
                fi
                ;;
        esac
    done < "$tmp_pairs"
    rm -f "$tmp_pairs"

    if [ -n "$best_space" ]; then
        printf '%s\tcwd-config\n' "$best_space"
        exit 0
    fi
fi

# Layer 4: cwd repo basename (only if cwd is inside a git repo)
if [ -n "$cwd" ] && [ -d "$cwd" ]; then
    repo_root="$(cd "$cwd" && git rev-parse --show-toplevel 2>/dev/null || true)"
    if [ -n "$repo_root" ]; then
        printf '%s\tcwd-repo\n' "$(basename "$repo_root")"
        exit 0
    fi
fi

# Layer 5: topic
if [ -n "$topic" ]; then
    printf '%s\ttopic\n' "$topic"
    exit 0
fi

# Layer 6: default
printf 'personal\tdefault\n'
