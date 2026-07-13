#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <receipt-path>" >&2
  exit 2
fi

receipt=$1
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
temp_root=$(mktemp -d)
fixture="$temp_root/fixture"
build_json="$temp_root/build.jsonl"
setup_output="$temp_root/setup.txt"
run_output="$temp_root/run.txt"
time_output="$temp_root/time.txt"

cleanup() {
  local system_temp=${TMPDIR:-/tmp}
  system_temp=${system_temp%/}
  case "$temp_root" in
    "$system_temp"/*|/tmp/*|/private/tmp/*) rm -rf -- "$temp_root" ;;
    *) echo "refusing to remove unexpected temporary path: $temp_root" >&2 ;;
  esac
}
trap cleanup EXIT

cd "$repo_root"
cargo test -p wenlan-core --lib --no-run --message-format=json >"$build_json"
test_binary=$(python3 - "$build_json" <<'PY'
import json
import pathlib
import sys

candidate = None
for line in pathlib.Path(sys.argv[1]).read_text().splitlines():
    record = json.loads(line)
    target = record.get("target", {})
    if (
        record.get("reason") == "compiler-artifact"
        and target.get("name") == "wenlan_core"
        and "lib" in target.get("kind", [])
        and record.get("profile", {}).get("test")
        and record.get("executable")
    ):
        candidate = record["executable"]
if candidate is None:
    raise SystemExit("wenlan-core lib test executable not found")
print(candidate)
PY
)

setup_test=lint::pages::diagnostic_scale_test::generate_diagnostic_scale_fixture
gate_test=lint::pages::diagnostic_scale_test::production_page_group_scale_gate
WENLAN_LINT_SCALE_FIXTURE="$fixture" "$test_binary" \
  "$setup_test" --ignored --exact --nocapture >"$setup_output" 2>&1

platform=$(uname -s)
if [[ "$platform" == Linux ]]; then
  time_args=(-v)
  hardware="$(uname -m); $(lscpu | awk -F: '/Model name/{sub(/^[[:space:]]+/, "", $2); print $2; exit}'); $(awk '/MemTotal/{print $0}' /proc/meminfo)"
elif [[ "$platform" == Darwin ]]; then
  time_args=(-l)
  hardware="$(uname -m); $(sysctl -n machdep.cpu.brand_string); memory_bytes=$(sysctl -n hw.memsize)"
else
  echo "unsupported timing platform: $platform" >&2
  exit 2
fi

set +e
WENLAN_LINT_SCALE_FIXTURE="$fixture" /usr/bin/time "${time_args[@]}" -o "$time_output" \
  "$test_binary" "$gate_test" --ignored --exact --nocapture >"$run_output" 2>&1
gate_status=$?
set -e

mkdir -p "$(dirname "$receipt")"
{
  echo "task=19 diagnostic scale and portability gate"
  echo "cwd=$repo_root"
  echo "head=$(git rev-parse HEAD)"
  echo "platform=$platform"
  echo "hardware=$hardware"
  echo "fixture_command=WENLAN_LINT_SCALE_FIXTURE=<temp>/fixture $test_binary $setup_test --ignored --exact --nocapture"
  echo "measured_command=WENLAN_LINT_SCALE_FIXTURE=<temp>/fixture /usr/bin/time ${time_args[*]} $test_binary $gate_test --ignored --exact --nocapture"
  echo "measured_region=shared snapshot open + Page scan + exact population assertions + production Page group + snapshot finish; fixture generation and Cargo build excluded"
  echo "model_embedder=not constructed or loaded"
  echo "exit_code=$gate_status"
  echo "setup_output:"
  cat "$setup_output"
  echo "measured_output:"
  cat "$run_output"
  echo "time_output:"
  cat "$time_output"
} >"$receipt"
cat "$receipt"

if [[ $gate_status -ne 0 ]]; then
  exit "$gate_status"
fi

if [[ "$platform" == Linux ]]; then
  python3 - "$time_output" <<'PY' | tee -a "$receipt"
import pathlib
import re
import sys

text = pathlib.Path(sys.argv[1]).read_text()
elapsed_match = re.search(r"Elapsed \(wall clock\) time .*: ([0-9:.]+)", text)
rss_match = re.search(r"Maximum resident set size \(kbytes\): ([0-9]+)", text)
if elapsed_match is None or rss_match is None:
    raise SystemExit("GNU time receipt is missing elapsed time or peak RSS")
parts = [float(part) for part in elapsed_match.group(1).split(":")]
elapsed = 0.0
for part in parts:
    elapsed = elapsed * 60.0 + part
rss_kib = int(rss_match.group(1))
print(f"LINUX_THRESHOLDS elapsed_seconds={elapsed:.3f} peak_rss_kib={rss_kib}")
if not elapsed < 5.000:
    raise SystemExit(f"elapsed threshold failed: {elapsed:.3f}s is not <5.000s")
if not rss_kib < 262144:
    raise SystemExit(f"RSS threshold failed: {rss_kib} KiB is not <262144 KiB")
PY
else
  python3 - "$time_output" <<'PY' | tee -a "$receipt"
import pathlib
import re
import sys

text = pathlib.Path(sys.argv[1]).read_text()
elapsed_match = re.search(r"\s*([0-9.]+) real", text)
rss_match = re.search(r"\s*([0-9]+)\s+maximum resident set size", text)
if elapsed_match is None or rss_match is None:
    raise SystemExit("macOS time receipt is missing elapsed time or peak RSS")
elapsed = float(elapsed_match.group(1))
rss_kib = int(rss_match.group(1)) // 1024
print(f"MACOS_RECEIPT elapsed_seconds={elapsed:.3f} peak_rss_kib={rss_kib}")
PY
fi
