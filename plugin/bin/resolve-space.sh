#!/usr/bin/env bash
# resolve-space.sh — Resolve the active Wenlan space from the 6-layer chain.
#
# Layers (highest to lowest priority):
#   1. --arg <space>                explicit override from skill caller
#   2. WENLAN_SPACE env var         per-shell pin
#   3. ~/.wenlan/spaces.toml        cwd-prefix mapping (longest prefix wins)
#      3.5. top-level `default` key  applies when no [[mapping]] matched
#   4. cwd repo basename            git rev-parse --show-toplevel
#   5. --topic <string>             conversation topic fallback
#   6. "personal"                   hard default
#
# Output: "<space>\t<source-layer>" on stdout.
# Source-layer values: arg | env | cwd-config | cwd-config-default | cwd-repo | topic | default
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

# Layer 1: explicit --arg override
arg_trimmed="$(printf '%s' "$arg" | tr -d '[:space:]')"
if [ -n "$arg_trimmed" ]; then
    printf '%s\targ\n' "$arg_trimmed"
    exit 0
fi

# Layer 2: WENLAN_SPACE env var (per-shell pin)
env_trimmed="$(printf '%s' "${WENLAN_SPACE:-}" | tr -d '[:space:]')"
if [ -n "$env_trimmed" ]; then
    printf '%s\tenv\n' "$env_trimmed"
    exit 0
fi

# Layer 3: cwd-config TOML mapping
# Reads file at $SPACES_FILE (override), else ~/.wenlan/spaces.toml.
# Parses [[mapping]] blocks; longest prefix matching $cwd wins.
spaces_file="${SPACES_FILE:-$HOME/.wenlan/spaces.toml}"
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
            sub(/[ \t]*$/, "")
            gsub(/"/, "")
            cur_prefix = $0
        }
        in_block && /^space[ \t]*=/ {
            sub(/^space[ \t]*=[ \t]*/, "")
            sub(/[ \t]*$/, "")
            gsub(/"/, "")
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

    # Look for a top-level `default = "..."` key.
    # `default` is not a valid key inside [[mapping]] blocks (only prefix/space are),
    # so we can match it without tracking block state.
    toml_default="$(awk '
        /^default[ \t]*=/ {
            sub(/^default[ \t]*=[ \t]*/, "")
            sub(/[ \t]*$/, "")
            gsub(/"/, "")
            print; exit
        }
    ' "$spaces_file" 2>/dev/null)"

    if [ -n "$best_space" ]; then
        printf '%s\tcwd-config\n' "$best_space"
        exit 0
    elif [ -n "$toml_default" ]; then
        printf '%s\tcwd-config-default\n' "$toml_default"
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
