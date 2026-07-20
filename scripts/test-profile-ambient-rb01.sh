#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${ROOT}/scripts/profile-ambient-rb01.sh"

preflight_lines=()
while IFS= read -r line; do
  preflight_lines[${#preflight_lines[@]}]="${line}"
done < <(/usr/bin/awk '/^preflight$/ { print NR }' "${TARGET}")
build_line="$(
  /usr/bin/awk '/^rtk proxy cargo test -j 2 -p wenlan-server rb01_profile_admission --lib --no-run --message-format=json / { print NR }' \
    "${TARGET}"
)"
inference_line="$(
  /usr/bin/awk '/scheduler::tests::rb01_profile_real_on_device_slice/ { print NR; exit }' \
    "${TARGET}"
)"

if [[ "${#preflight_lines[@]}" -ne 2 ]]; then
  echo "expected exactly two real-profile preflight calls, found ${#preflight_lines[@]}" >&2
  exit 1
fi

if [[ -z "${build_line}" || -z "${inference_line}" ]]; then
  echo "could not locate the bounded build or real inference command" >&2
  exit 1
fi

lock_line="$(
  /usr/bin/awk '/^acquire_profile_lock$/ { print NR; exit }' "${TARGET}"
)"
if [[ -z "${lock_line}" ]] || ! (( lock_line < preflight_lines[0] )); then
  echo "expected an exclusive profile lock before the first real preflight" >&2
  exit 1
fi

success_guard_line="$(
  /usr/bin/awk '/^if \(\( status == 0 \)\); then$/ { print NR; exit }' "${TARGET}"
)"
measured_cooldown_line="$(
  /usr/bin/awk '/^  cooldown_from_report .* \|\| true$/ { print NR; exit }' "${TARGET}"
)"
if [[ -z "${success_guard_line}" || -z "${measured_cooldown_line}" ]] ||
  ! (( success_guard_line < measured_cooldown_line )); then
  echo "expected only a successful child to narrow the fail-safe cooldown" >&2
  exit 1
fi

if ! /usr/bin/grep -q '"rss_peak_during_slice_bytes"' \
  "${ROOT}/crates/wenlan-server/src/scheduler.rs"; then
  echo "expected the real profile JSON to report observed peak slice RSS" >&2
  exit 1
fi

if ! (( preflight_lines[0] < build_line &&
        build_line < preflight_lines[1] &&
        preflight_lines[1] < inference_line )); then
  echo "unsafe order: expected preflight -> bounded build -> preflight -> inference" >&2
  exit 1
fi

headroom_calls="$(
  /usr/bin/awk '/^  check_model_headroom( |$)/ { count += 1 } END { print count + 0 }' \
    "${TARGET}"
)"
if [[ "${headroom_calls}" -ne 1 ]]; then
  echo "expected preflight to enforce model working-set headroom exactly once" >&2
  exit 1
fi

if ! /usr/bin/grep -q '"report_elapsed_ms"' "${TARGET}" ||
  ! /usr/bin/grep -q 'slice_seconds \* 19' "${TARGET}" ||
  ! /usr/bin/grep -q 'TIMEOUT_SECS \* 19' "${TARGET}"; then
  echo "expected cooldown to use 19x measured job time with a timeout-sized fail-safe" >&2
  exit 1
fi

if ! /usr/bin/grep -q '^trap - INT TERM EXIT$' "${TARGET}" ||
  ! /usr/bin/grep -q '^exit "${status}"$' "${TARGET}"; then
  echo "expected the real profile to finalize once and propagate the child status" >&2
  exit 1
fi

for provenance_key in \
  source_scheduler_sha256 \
  source_db_sha256 \
  source_profiler_sha256 \
  cargo_lock_sha256 \
  test_binary_sha256 \
  model_blob_sha256 \
  model_resolved_path \
  git_worktree_diff_sha256; do
  if ! /usr/bin/grep -q "^  echo \"${provenance_key}=" "${TARGET}"; then
    echo "expected manifest provenance key ${provenance_key}" >&2
    exit 1
  fi
done

if ! /usr/bin/grep -q -- '--message-format=json' "${TARGET}" ||
  ! /usr/bin/grep -q 'rtk proxy "${test_binary}"' "${TARGET}"; then
  echo "expected the measured child to be the hashed Cargo test executable" >&2
  exit 1
fi

if /usr/bin/grep -q '/usr/bin/jq' "${TARGET}" ||
  ! /usr/bin/grep -q '^JQ_BIN="$(command -v jq || true)"$' "${TARGET}" ||
  ! /usr/bin/grep -q '^  "${JQ_BIN}" -r ' "${TARGET}"; then
  echo "expected jq to be discovered from PATH instead of a Homebrew-incompatible fixed path" >&2
  exit 1
fi

if ! /usr/bin/grep -q '^    rtk proxy git diff --binary HEAD$' "${TARGET}" ||
  ! /usr/bin/grep -q '^    rtk proxy git diff --binary --cached HEAD$' "${TARGET}"; then
  echo "expected provenance to hash exact unfiltered git diff bytes" >&2
  exit 1
fi

if ! /usr/bin/grep -q '^  document|entity|page-growth|reconcile|citation)$' "${TARGET}"; then
  echo "expected the real profiler CLI to include the Page Growth no-match lane" >&2
  exit 1
fi

echo "profile order ok: preflight -> bounded build -> preflight -> inference"
