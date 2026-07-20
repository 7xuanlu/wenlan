#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODEL_CACHE="${HOME}/.cache/huggingface/hub/models--unsloth--Qwen3-4B-Instruct-2507-GGUF/snapshots"
MODEL_FILE="Qwen3-4B-Instruct-2507-Q4_K_M.gguf"
COOLDOWN_STATE="/private/tmp/wenlan-rb01-profile-next-allowed"
PROFILE_LOCK="/private/tmp/wenlan-rb01-profile.lock"
TIMEOUT_SECS=300
JQ_BIN="$(command -v jq || true)"
child_pid=""
evidence_dir=""
model_path=""
model_resolved_path=""
model_blob_sha256=""
build_metadata=""
test_binary=""
test_binary_sha256=""
cargo_lock_sha256=""
git_worktree_diff_sha256=""
lock_held=0

usage() {
  echo "usage: scripts/profile-ambient-rb01.sh [--preflight-only|document|entity|page-growth|reconcile|citation]"
}

thermal_state() {
  /usr/bin/swift -e \
    'import Foundation; print(ProcessInfo.processInfo.thermalState.rawValue)' \
    2>/dev/null
}

memory_free_percent() {
  /usr/bin/memory_pressure -Q |
    /usr/bin/awk -F': ' '/System-wide memory free percentage/ {gsub("%", "", $2); print $2}'
}

sha256_file() {
  /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'
}

acquire_profile_lock() {
  if ! /bin/mkdir "${PROFILE_LOCK}" 2>/dev/null; then
    echo "refusing profile: another RB-01 profiler owns ${PROFILE_LOCK}" >&2
    return 1
  fi
  lock_held=1
  echo "$$" >"${PROFILE_LOCK}/owner_pid"
}

release_profile_lock() {
  if (( lock_held == 0 )); then
    return
  fi
  /bin/rm -f "${PROFILE_LOCK}/owner_pid"
  /bin/rmdir "${PROFILE_LOCK}" 2>/dev/null || true
  lock_held=0
}

check_cached_model() {
  model_path="$(
    /usr/bin/find -L "${MODEL_CACHE}" -name "${MODEL_FILE}" -type f -print -quit 2>/dev/null
  )"
  if [[ -z "${model_path}" ]]; then
    echo "refusing profile: cached ${MODEL_FILE} was not found; this script never downloads models" >&2
    return 1
  fi
}

check_model_headroom() {
  local free_percent="${1}"
  local total_bytes model_bytes headroom
  total_bytes="$(
    /usr/bin/memory_pressure -Q |
      /usr/bin/awk '/^The system has [0-9]+/ { print $4; exit }'
  )"
  model_bytes="$(/usr/bin/stat -Lf '%z' "${model_path}" 2>/dev/null || true)"
  if [[ ! "${total_bytes}" =~ ^[0-9]+$ ]] ||
    [[ ! "${model_bytes}" =~ ^[0-9]+$ ]]; then
    echo "refusing profile: model working-set headroom is unavailable" >&2
    return 1
  fi

  if ! headroom="$(
    /usr/bin/awk \
      -v total="${total_bytes}" \
      -v free_percent="${free_percent}" \
      -v model="${model_bytes}" '
        BEGIN {
          policy_floor = total * 0.15
          if (policy_floor < 2147483648) {
            policy_floor = 2147483648
          }
          free_bytes = total * free_percent / 100
          required = model + policy_floor
          printf "%.0f %.0f", free_bytes, required
          exit !(free_bytes >= required)
        }
      '
  )"; then
    echo "refusing profile: estimated free/required bytes are ${headroom}; required includes cached model plus scheduler memory floor" >&2
    return 1
  fi
}

check_cooldown() {
  local now next remaining
  now="$(date +%s)"
  if [[ ! -f "${COOLDOWN_STATE}" ]]; then
    return 0
  fi
  next="$(/bin/cat "${COOLDOWN_STATE}")"
  if [[ ! "${next}" =~ ^[0-9]+$ ]] || (( now >= next )); then
    return 0
  fi
  remaining=$((next - now))
  echo "refusing profile: prior real slice still owns the cooldown for ${remaining}s" >&2
  return 1
}

arm_failsafe_cooldown() {
  local cooldown_seconds=$((TIMEOUT_SECS * 19))
  echo $(( $(date +%s) + cooldown_seconds )) >"${COOLDOWN_STATE}"
  if [[ -n "${evidence_dir}" ]]; then
    {
      echo "cooldown_source=failsafe_timeout"
      echo "cooldown_seconds=${cooldown_seconds}"
    } >>"${evidence_dir}/manifest.txt"
  fi
}

cooldown_from_report() {
  local raw_log="${1}"
  local report_elapsed_ms slice_seconds cooldown_seconds
  report_elapsed_ms="$(
    /usr/bin/awk '
      match($0, /"report_elapsed_ms":[0-9]+/) {
        value = substr($0, RSTART, RLENGTH)
        sub(/^.*:/, "", value)
        latest = value
      }
      END {
        if (latest != "") {
          print latest
        }
      }
    ' "${raw_log}"
  )"
  if [[ ! "${report_elapsed_ms}" =~ ^[0-9]+$ ]]; then
    return 1
  fi

  slice_seconds=$(( (report_elapsed_ms + 999) / 1000 ))
  cooldown_seconds=$((slice_seconds * 19))
  if (( cooldown_seconds < 600 )); then
    cooldown_seconds=600
  fi
  echo $(( $(date +%s) + cooldown_seconds )) >"${COOLDOWN_STATE}"
  {
    echo "cooldown_source=measured_job_elapsed"
    echo "report_elapsed_ms=${report_elapsed_ms}"
    echo "cooldown_seconds=${cooldown_seconds}"
  } >>"${evidence_dir}/manifest.txt"
}

check_two_cpu_samples() {
  local samples count value
  samples="$(
    /usr/bin/top -l 3 -n 0 -s 30 |
      /usr/bin/awk '
        /CPU usage/ {
          for (i = 1; i <= NF; i++) {
            if (index($i, "idle") > 0) {
              idle = $(i - 1)
              gsub("%", "", idle)
              print 100 - idle
            }
          }
        }
      ' |
      /usr/bin/tail -n 2
  )"
  count=0
  while IFS= read -r value; do
    [[ -z "${value}" ]] && continue
    count=$((count + 1))
    if ! /usr/bin/awk -v value="${value}" 'BEGIN { exit !(value <= 20.0) }'; then
      echo "refusing profile: aggregate CPU sample ${count}/2 was ${value}% (>20%)" >&2
      return 1
    fi
  done <<<"${samples}"
  if (( count != 2 )); then
    echo "refusing profile: expected two aggregate CPU samples, observed ${count}" >&2
    return 1
  fi
  echo "${samples}"
}

preflight() {
  local thermal memory cpu_samples
  check_cached_model
  check_cooldown

  thermal="$(thermal_state)"
  if [[ "${thermal}" != "0" ]]; then
    echo "refusing profile: macOS thermal state is ${thermal:-unavailable}, expected 0 (nominal)" >&2
    return 1
  fi

  memory="$(memory_free_percent)"
  if [[ -z "${memory}" ]] ||
    ! /usr/bin/awk -v value="${memory}" 'BEGIN { exit !(value >= 15.0) }'; then
    echo "refusing profile: free memory is ${memory:-unavailable}%, expected at least 15%" >&2
    return 1
  fi
  check_model_headroom "${memory}"

  cpu_samples="$(check_two_cpu_samples)"
  echo "preflight_ok thermal_state=${thermal} memory_free_percent=${memory} cpu_percent_samples=$(echo "${cpu_samples}" | tr '\n' ',')"
}

finish_manifest() {
  local status="${1:-130}"
  if [[ -n "${evidence_dir}" ]]; then
    {
      echo "exit_status=${status}"
      echo "finished_at_epoch=$(date +%s)"
      echo "thermal_state_after=$(thermal_state || true)"
      echo "memory_free_percent_after=$(memory_free_percent || true)"
    } >>"${evidence_dir}/manifest.txt"
  fi
}

cleanup() {
  local status=$?
  if [[ -n "${child_pid}" ]] && kill -0 "${child_pid}" 2>/dev/null; then
    kill -TERM "${child_pid}" 2>/dev/null || true
    wait "${child_pid}" 2>/dev/null || true
  fi
  finish_manifest "${status}"
  if [[ -n "${build_metadata}" ]]; then
    /bin/rm -f "${build_metadata}"
  fi
  release_profile_lock
}

trap cleanup INT TERM EXIT

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 2
fi

case "$1" in
  --help|-h)
    usage
    exit 0
    ;;
  --preflight-only)
    preflight
    exit 0
    ;;
  document|entity|page-growth|reconcile|citation)
    lane="$1"
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac

cd "${ROOT}"
acquire_profile_lock
if [[ -z "${JQ_BIN}" ]]; then
  echo "refusing profile: jq is required to identify the exact Cargo test executable" >&2
  exit 1
fi
# Refuse before compiling so the profiler itself never adds build load to an
# already-busy foreground session.
preflight
build_metadata="$(/usr/bin/mktemp /private/tmp/wenlan-rb01-build.XXXXXX)"
rtk proxy cargo test -j 2 -p wenlan-server rb01_profile_admission --lib --no-run --message-format=json >"${build_metadata}"
test_binary="$(
  "${JQ_BIN}" -r '
    select(
      .reason == "compiler-artifact"
      and .target.name == "wenlan_server"
      and .profile.test == true
      and .executable != null
    )
    | .executable
  ' "${build_metadata}" |
    /usr/bin/tail -n 1
)"
if [[ -z "${test_binary}" || ! -x "${test_binary}" ]]; then
  echo "refusing profile: Cargo did not produce the expected wenlan-server test binary" >&2
  exit 1
fi
test_binary_sha256="$(sha256_file "${test_binary}")"
cargo_lock_sha256="$(sha256_file Cargo.lock)"
model_resolved_path="$(
  /usr/bin/perl -MCwd=realpath -e 'print realpath($ARGV[0])' "${model_path}"
)"
model_blob_sha256="$(/usr/bin/basename "${model_resolved_path}")"
if [[ ! "${model_blob_sha256}" =~ ^[0-9a-f]{64}$ ]]; then
  echo "refusing profile: cached model is not backed by a content-addressed blob" >&2
  exit 1
fi
git_worktree_diff_sha256="$(
  {
    rtk proxy git diff --binary HEAD
    rtk proxy git diff --binary --cached HEAD
  } | /usr/bin/shasum -a 256 | /usr/bin/awk '{print $1}'
)"
# Compilation can change both CPU and thermal state. Reacquire the full
# two-sample admission window before loading Metal or starting inference.
preflight

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
evidence_dir="/private/tmp/wenlan-rb01-profile-${stamp}-${lane}"
/bin/mkdir -p "${evidence_dir}"
{
  echo "lane=${lane}"
  echo "started_at_epoch=$(date +%s)"
  echo "git_head=$(rtk git rev-parse HEAD)"
  echo "source_scheduler_sha256=$(sha256_file crates/wenlan-server/src/scheduler.rs)"
  echo "source_db_sha256=$(sha256_file crates/wenlan-core/src/db.rs)"
  echo "source_profiler_sha256=$(sha256_file scripts/profile-ambient-rb01.sh)"
  echo "cargo_lock_sha256=${cargo_lock_sha256}"
  echo "test_binary_path=${test_binary}"
  echo "test_binary_sha256=${test_binary_sha256}"
  echo "model_resolved_path=${model_resolved_path}"
  echo "model_blob_sha256=${model_blob_sha256}"
  echo "git_worktree_diff_sha256=${git_worktree_diff_sha256}"
  echo "thermal_state_before=$(thermal_state)"
  echo "memory_free_percent_before=$(memory_free_percent)"
  echo "timeout_secs=${TIMEOUT_SECS}"
} >"${evidence_dir}/manifest.txt"

# Arm a conservative timeout-sized recovery window before the child can load
# the model. An interrupt or missing JSON result leaves this fail-safe intact.
arm_failsafe_cooldown

set +e
(
  WENLAN_RB01_PROFILE=1 \
    WENLAN_RB01_LANE="${lane}" \
    /usr/bin/perl -e 'alarm shift @ARGV; exec @ARGV' \
    "${TIMEOUT_SECS}" \
    rtk proxy "${test_binary}" \
    scheduler::tests::rb01_profile_real_on_device_slice \
    --ignored --exact --nocapture --test-threads=1
) >"${evidence_dir}/raw.log" 2>&1 &
child_pid=$!
wait "${child_pid}"
status=$?
child_pid=""
set -e

# A complete report narrows the fail-safe to the production thermal policy:
# max(10 minutes, 19x the measured scheduler job duration).
if (( status == 0 )); then
  cooldown_from_report "${evidence_dir}/raw.log" || true
fi
finish_manifest "${status}"
if [[ -n "${build_metadata}" ]]; then
  /bin/rm -f "${build_metadata}"
  build_metadata=""
fi
release_profile_lock
trap - INT TERM EXIT

/bin/cat "${evidence_dir}/raw.log"
echo "evidence_dir=${evidence_dir}"
exit "${status}"
