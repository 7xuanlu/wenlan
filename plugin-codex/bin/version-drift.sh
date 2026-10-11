#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Compare the running Wenlan daemon version against a plugin manifest version.
#
# Usage:
#   version-drift.sh --health-url <url> --manifest <plugin.json> [--ignore-patch]
#
# Reads {"version": ...} from the health endpoint (curl -fsS -m 3) and from the
# manifest. Build metadata (`+g<sha>` on a daemon, `+codex` on a manifest) is
# stripped before comparing; components compare numerically, never as text.
# --ignore-patch compares only major.minor.
#
# stdout, exactly one line:
#   daemon=<v> expected=<v> release=<v> result=same|older|newer|unknown
# where `release` is the manifest version without build metadata and `older`
# means the daemon is older than the plugin. Exit code: 0 same, 2 older,
# 3 newer, 4 unknown (health down, a version empty, manifest unreadable),
# 64 usage error (unknown flag).
set -u

HEALTH_URL=""
MANIFEST=""
IGNORE_PATCH=0
while [ $# -gt 0 ]; do
  case "$1" in
    --health-url) HEALTH_URL="${2-}"; shift 2 || break ;;
    --manifest) MANIFEST="${2-}"; shift 2 || break ;;
    --ignore-patch) IGNORE_PATCH=1; shift ;;
    *) echo "usage: version-drift.sh --health-url <url> --manifest <plugin.json> [--ignore-patch]" >&2; exit 64 ;;
  esac
done

json_version() {
  sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1
}

DAEMON_VER=""
EXPECTED_VER=""
if [ -n "$HEALTH_URL" ]; then
  DAEMON_VER=$(curl -fsS -m 3 "$HEALTH_URL" 2>/dev/null | json_version)
fi
if [ -n "$MANIFEST" ] && [ -r "$MANIFEST" ]; then
  EXPECTED_VER=$(json_version <"$MANIFEST")
fi
RELEASE_VER="${EXPECTED_VER%%+*}"
DAEMON_REL="${DAEMON_VER%%+*}"

emit() { # $1 result, $2 exit code
  printf 'daemon=%s expected=%s release=%s result=%s\n' "$DAEMON_VER" "$EXPECTED_VER" "$RELEASE_VER" "$1"
  exit "$2"
}

# Print "major minor patch" (numeric; a prerelease tail is dropped); fail if not numeric.
parts() {
  local v="${1%%-*}" major minor patch
  major="${v%%.*}"
  if [ "$v" = "$major" ]; then minor=0; patch=0; else
    v="${v#*.}"; minor="${v%%.*}"
    if [ "$v" = "$minor" ]; then patch=0; else patch="${v#*.}"; patch="${patch%%.*}"; fi
  fi
  case "$major$minor$patch" in ''|*[!0-9]*) return 1 ;; esac
  [ -n "$major" ] && [ -n "$minor" ] && [ -n "$patch" ] || return 1
  printf '%s %s %s' "$major" "$minor" "$patch"
}

if [ -z "$DAEMON_REL" ] || [ -z "$RELEASE_VER" ]; then emit unknown 4; fi
D=$(parts "$DAEMON_REL") || emit unknown 4
E=$(parts "$RELEASE_VER") || emit unknown 4
# shellcheck disable=SC2086  # intentional word splitting of "major minor patch"
set -- $D $E
d_major=$1 d_minor=$2 d_patch=$3 e_major=$4 e_minor=$5 e_patch=$6
if [ "$IGNORE_PATCH" -eq 1 ]; then d_patch=0; e_patch=0; fi

for pair in "$d_major $e_major" "$d_minor $e_minor" "$d_patch $e_patch"; do
  # shellcheck disable=SC2086
  set -- $pair
  if [ "$1" -lt "$2" ]; then emit older 2; fi
  if [ "$1" -gt "$2" ]; then emit newer 3; fi
done
emit same 0
