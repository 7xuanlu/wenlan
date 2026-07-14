#!/usr/bin/env bash
set -euo pipefail
umask 077

usage() {
    cat <<'EOF'
Usage: lint-maintenance-probe.sh --db PATH --pages-root PATH [options]

Capture a read-only Wenlan maintenance snapshot and redacted aggregate counts.
Raw artifacts are written outside git worktrees under:
  ${REPO_DATA_ROOT:-$HOME/.local/share/repo-data}/wenlan/lint-maintenance/<sha>/<run-id>/

Options:
  --wenlan-bin PATH       Capture canonical General and Deep lint receipts.
  --output-root PATH      Override REPO_DATA_ROOT.
  -h, --help              Show this help.
EOF
}

fail() {
    echo "lint-maintenance-probe: $*" >&2
    exit 2
}

db=""
pages_root=""
wenlan_bin=""
output_root="${REPO_DATA_ROOT:-$HOME/.local/share/repo-data}"
test_after_first_receipt_hook=""

while (($#)); do
    case "$1" in
        --db)
            db=${2:-}
            shift 2
            ;;
        --pages-root)
            pages_root=${2:-}
            shift 2
            ;;
        --wenlan-bin)
            wenlan_bin=${2:-}
            shift 2
            ;;
        --output-root)
            output_root=${2:-}
            shift 2
            ;;
        --test-after-first-receipt-hook)
            test_after_first_receipt_hook=${2:-}
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

[[ -n "$db" && -f "$db" ]] || fail "--db must name an existing database"
[[ -n "$pages_root" && -d "$pages_root" ]] || fail "--pages-root must name an existing directory"
if [[ -n "$wenlan_bin" ]]; then
    [[ -x "$wenlan_bin" && ! -d "$wenlan_bin" ]] \
        || fail "--wenlan-bin must name an executable file"
fi
if [[ -n "$test_after_first_receipt_hook" ]]; then
    [[ -x "$test_after_first_receipt_hook" && ! -d "$test_after_first_receipt_hook" ]] \
        || fail "--test-after-first-receipt-hook must name an executable file"
fi

for tool in git jq realpath shasum sqlite3 tar; do
    command -v "$tool" >/dev/null 2>&1 || fail "$tool is required"
done

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
sql_file="$repo_root/scripts/lint-maintenance-probe.sql"
[[ -f "$sql_file" ]] || fail "aggregate SQL is missing: $sql_file"

absolute_lexical() {
    local path=$1
    if [[ "$path" == /* ]]; then
        printf '%s\n' "$path"
    else
        printf '%s/%s\n' "$PWD" "$path"
    fi
}

reject_unsafe_components() {
    local label=$1 path=$2 part
    [[ ! -L "$path" ]] || fail "$label symlink is not allowed: $path"
    path=$(absolute_lexical "$path")
    IFS='/' read -r -a parts <<<"$path"
    for part in "${parts[@]}"; do
        [[ -z "$part" || "$part" == "." ]] && continue
        [[ "$part" != ".." ]] || fail "$label path escape is not allowed"
    done
}

reject_unsafe_components "database" "$db"
reject_unsafe_components "Pages root" "$pages_root"
reject_unsafe_components "output root" "$output_root"
[[ -z "$wenlan_bin" ]] || reject_unsafe_components "wenlan binary" "$wenlan_bin"
[[ -z "$test_after_first_receipt_hook" ]] \
    || reject_unsafe_components "test receipt hook" "$test_after_first_receipt_hook"

db=$(realpath "$db")
pages_root=$(realpath "$pages_root")
output_root_lexical=$(absolute_lexical "$output_root")
output_probe="$output_root_lexical"
while [[ ! -e "$output_probe" ]]; do
    parent=$(dirname "$output_probe")
    [[ "$parent" != "$output_probe" ]] || break
    output_probe=$parent
done
if output_git_root=$(git -C "$output_probe" rev-parse --show-toplevel 2>/dev/null); then
    fail "output root must be outside git worktrees: $output_git_root"
fi
mkdir -p "$output_root_lexical"
output_root=$(realpath "$output_root_lexical")
[[ "$db" != *"'"* && "$output_root" != *"'"* ]] \
    || fail "single quotes are not supported in database or output paths"

page_tree_sha256() {
    local root=$1 symlink
    symlink=$(find "$root" -type l -print -quit)
    [[ -z "$symlink" ]] || fail "Page tree symlink is not allowed: $symlink"
    (
        cd "$root"
        find . -type f -print0 \
            | LC_ALL=C sort -z \
            | while IFS= read -r -d '' file; do
                printf '%s\0' "${file#./}"
                shasum -a 256 "$file" | awk '{print $1}'
            done
    ) | shasum -a 256 | awk '{print $1}'
}

file_sha256() {
    shasum -a 256 "$1" | awk '{print $1}'
}

sqlite_backup() {
    local source=$1 target=$2
    sqlite3 "$source" ".backup '$target'"
    [[ -s "$target" ]] || fail "SQLite backup is empty: $target"
    chmod 600 "$target"
}

run_lint_bounded() {
    local stdout=$1 stderr=$2
    shift 2
    local pid code=0
    "$wenlan_bin" "$@" >"$stdout" 2>"$stderr" &
    pid=$!
    for _ in $(seq 1 600); do
        if ! kill -0 "$pid" 2>/dev/null; then
            wait "$pid" || code=$?
            chmod 600 "$stdout" "$stderr"
            return "$code"
        fi
        sleep 0.1
    done
    kill "$pid" >/dev/null 2>&1 || true
    wait "$pid" 2>/dev/null || true
    chmod 600 "$stdout" "$stderr"
    return 124
}

commit_sha=$(git -C "$repo_root" rev-parse HEAD)
if [[ -n $(git -C "$repo_root" status --porcelain) ]]; then
    dirty=true
else
    dirty=false
fi
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$-$RANDOM"
run_dir="$output_root/wenlan/lint-maintenance/$commit_sha/$run_id"
mkdir -p "$run_dir"
chmod 700 "$run_dir"

started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
pages_before=$(page_tree_sha256 "$pages_root")
db_before="$run_dir/db-before.sqlite"
db_after="$run_dir/db-after.sqlite"
sqlite_backup "$db" "$db_before"
db_before_sha=$(file_sha256 "$db_before")

pages_snapshot="$run_dir/pages-snapshot.tar"
tar -C "$pages_root" -cf "$pages_snapshot" .
chmod 600 "$pages_snapshot"
pages_verify_dir="$run_dir/.pages-snapshot-verify"
mkdir -p "$pages_verify_dir"
chmod 700 "$pages_verify_dir"
tar -C "$pages_verify_dir" -xf "$pages_snapshot"
pages_snapshot_receipt=$(page_tree_sha256 "$pages_verify_dir")
rm -rf -- "$pages_verify_dir"

if [[ -n "$test_after_first_receipt_hook" ]]; then
    "$test_after_first_receipt_hook"
fi

required_tables=(
    memories entities relations pages page_sources page_evidence page_links
    enrichment_steps source_sync_state document_enrichment_queue
)
required_columns=(
    memories:id memories:source memories:source_id memories:chunk_index
    memories:entity_id memories:content_hash memories:pending_revision
    memories:supersedes memories:episode_of entities:id relations:from_entity
    relations:to_entity pages:id pages:entity_id pages:source_memory_ids
    page_sources:memory_source_id page_evidence:source_kind
    page_evidence:locator page_links:target_page_id enrichment_steps:source_id
    source_sync_state:source_id source_sync_state:file_path
    document_enrichment_queue:source_id document_enrichment_queue:file_path
    document_enrichment_queue:status
)
missing_tables=()
for table in "${required_tables[@]}"; do
    exists=$(sqlite3 -batch -noheader "$db_before" \
        "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='$table';")
    [[ "$exists" == 1 ]] || missing_tables+=("$table")
done
missing_columns=()
for spec in "${required_columns[@]}"; do
    table=${spec%%:*}
    column=${spec#*:}
    exists=$(sqlite3 -batch -noheader "$db_before" \
        "SELECT COUNT(*) FROM pragma_table_info('$table') WHERE name='$column';")
    [[ "$exists" == 1 ]] || missing_columns+=("$spec")
done

aggregates="$run_dir/aggregates.json"
aggregate_status=ok
if ((${#missing_tables[@]} == 0 && ${#missing_columns[@]} == 0)); then
    aggregate_error="$run_dir/aggregates.err"
    aggregate_tsv="$run_dir/aggregate-counts.tsv"
    metric_names="foreign_key_violations page_sources_missing_owner page_evidence_memory_missing_owner pages_dangling_entity pending_revision_missing_target enrichment_steps_missing_owner legacy_page_sources_missing_owner relation_self_edges episodes_missing_parent broken_nonnull_page_links done_queue_missing_sync_receipt multi_chunk_memory_sources content_hash_missing_heads"
    if ! sqlite3 -batch -noheader "$db_before" <"$sql_file" 2>"$aggregate_error" \
        | awk -v names="$metric_names" '
            BEGIN {
                n = split(names, ordered, " ")
                for (i = 1; i <= n; i++) counts[ordered[i]] = 0
            }
            { if ($1 in counts) counts[$1] += 1 }
            END {
                for (i = 1; i <= n; i++) {
                    print ordered[i] "\t" counts[ordered[i]]
                }
            }
        ' >"$aggregate_tsv"; then
        aggregate_status=failed
        jq -n '{status:"not_applicable", reason:"aggregate_query_failed"}' >"$aggregates"
    else
        jq -Rn '
            [inputs | split("\t") | {(.[0]): (.[1] | tonumber)}]
            | add
        ' <"$aggregate_tsv" >"$aggregates"
    fi
    chmod 600 "$aggregate_error" "$aggregate_tsv"
else
    aggregate_status=not_applicable
    jq -n '{status:"not_applicable", reason:"required_schema_missing"}' >"$aggregates"
fi
chmod 600 "$aggregates"

count_oracle="$run_dir/count-oracle.json"
count_oracle_json='{}'
if ((${#missing_tables[@]} == 0 && ${#missing_columns[@]} == 0)); then
    count_oracle_status=match
    for table in memories entities pages; do
        scalar_count=$(sqlite3 -batch -noheader "$db_before" "SELECT COUNT(*) FROM $table;")
        enumerated_count=$(sqlite3 -batch -noheader "$db_before" "SELECT id FROM $table;" | wc -l | tr -d ' ')
        [[ "$scalar_count" == "$enumerated_count" ]] || count_oracle_status=mismatch
        count_oracle_json=$(jq -c \
            --arg table "$table" \
            --argjson scalar "$scalar_count" \
            --argjson enumerated "$enumerated_count" \
            '. + {($table): {scalar_count: $scalar, enumerated_count: $enumerated}}' \
            <<<"$count_oracle_json")
    done
else
    count_oracle_status=not_applicable
fi
jq -n \
    --arg status "$count_oracle_status" \
    --argjson tables "$count_oracle_json" \
    '{status: $status, tables: $tables}' >"$count_oracle"
chmod 600 "$count_oracle"

general_attempted=false
general_exit=null
deep_attempted=false
deep_exit=null
if [[ -n "$wenlan_bin" ]]; then
    general_attempted=true
    set +e
    run_lint_bounded "$run_dir/lint-general.json" "$run_dir/lint-general.err" \
        --format json lint
    general_exit=$?
    deep_attempted=true
    run_lint_bounded "$run_dir/lint-deep.json" "$run_dir/lint-deep.err" \
        --format json lint --profile deep
    deep_exit=$?
    set -e
fi

sqlite_backup "$db" "$db_after"
db_after_sha=$(file_sha256 "$db_after")
pages_after=$(page_tree_sha256 "$pages_root")

complete=true
reason=stable_snapshot
if [[ "$db_before_sha" != "$db_after_sha" || "$pages_before" != "$pages_after" ]]; then
    complete=false
    reason=inconsistent_snapshot
elif [[ "$pages_snapshot_receipt" != "$pages_before" ]]; then
    complete=false
    reason=page_snapshot_mismatch
elif [[ "$count_oracle_status" == mismatch ]]; then
    complete=false
    reason=count_oracle_mismatch
elif [[ "$aggregate_status" == failed ]]; then
    complete=false
    reason=probe_failure
elif ((${#missing_tables[@]} > 0 || ${#missing_columns[@]} > 0)); then
    complete=false
    reason=schema_unsupported
elif [[ "$general_attempted" == true && "$general_exit" -eq 2 ]]; then
    complete=false
    reason=general_lint_incomplete
elif [[ "$deep_attempted" == true && "$deep_exit" -eq 2 ]]; then
    complete=false
    reason=deep_lint_incomplete
elif [[ "$general_attempted" == true && "$general_exit" -gt 2 ]]; then
    complete=false
    reason=general_lint_execution_failure
elif [[ "$deep_attempted" == true && "$deep_exit" -gt 2 ]]; then
    complete=false
    reason=deep_lint_execution_failure
fi

artifacts_json='[]'
while IFS= read -r artifact; do
    relative=${artifact#"$run_dir/"}
    sha=$(file_sha256 "$artifact")
    bytes=$(wc -c <"$artifact" | tr -d ' ')
    artifacts_json=$(jq -c \
        --arg path "$relative" \
        --arg sha256 "$sha" \
        --argjson bytes "$bytes" \
        '. + [{path:$path, sha256:$sha256, bytes:$bytes}]' \
        <<<"$artifacts_json")
done < <(find "$run_dir" -maxdepth 1 -type f ! -name manifest.json | LC_ALL=C sort)

if ((${#missing_tables[@]} > 0)); then
    missing_tables_json=$(printf '%s\n' "${missing_tables[@]}" \
        | jq -Rsc 'split("\n") | map(select(length > 0))')
else
    missing_tables_json='[]'
fi
if ((${#missing_columns[@]} > 0)); then
    missing_columns_json=$(printf '%s\n' "${missing_columns[@]}" \
        | jq -Rsc 'split("\n") | map(select(length > 0))')
else
    missing_columns_json='[]'
fi
finished_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
manifest="$run_dir/manifest.json"
jq -n \
    --argjson schema_version 1 \
    --argjson probe_version 1 \
    --arg commit "$commit_sha" \
    --argjson dirty "$dirty" \
    --arg run_id "$run_id" \
    --arg started_at "$started_at" \
    --arg finished_at "$finished_at" \
    --arg db_before "$db_before_sha" \
    --arg db_after "$db_after_sha" \
    --arg pages_before "$pages_before" \
    --arg pages_after "$pages_after" \
    --arg pages_snapshot "$pages_snapshot_receipt" \
    --arg aggregate_status "$aggregate_status" \
    --arg count_oracle_status "$count_oracle_status" \
    --argjson missing_tables "$missing_tables_json" \
    --argjson missing_columns "$missing_columns_json" \
    --argjson general_attempted "$general_attempted" \
    --argjson general_exit "$general_exit" \
    --argjson deep_attempted "$deep_attempted" \
    --argjson deep_exit "$deep_exit" \
    --argjson complete "$complete" \
    --arg reason "$reason" \
    --argjson artifacts "$artifacts_json" \
    '{
        schema_version: $schema_version,
        probe_version: $probe_version,
        git: {commit: $commit, dirty: $dirty},
        run_id: $run_id,
        started_at: $started_at,
        finished_at: $finished_at,
        snapshot_method: "sqlite_backup_plus_page_archive_receipt",
        source: {
            database: {before_sha256: $db_before, after_sha256: $db_after},
            pages: {
                before_sha256: $pages_before,
                snapshot_sha256: $pages_snapshot,
                after_sha256: $pages_after
            }
        },
        schema: {
            aggregate_status: $aggregate_status,
            missing_tables: $missing_tables,
            missing_columns: $missing_columns
        },
        count_oracle: {status: $count_oracle_status},
        lint: {
            general: {attempted: $general_attempted, exit_code: $general_exit},
            deep: {attempted: $deep_attempted, exit_code: $deep_exit}
        },
        complete: $complete,
        reason: $reason,
        artifacts: $artifacts
    }' >"$manifest"
chmod 600 "$manifest"

printf '%s\n' \
    "manifest=$manifest" \
    "complete=$complete" \
    "reason=$reason" \
    "page_sources_missing_owner=$(jq -r '.page_sources_missing_owner // "not_applicable"' "$aggregates")" \
    "page_evidence_memory_missing_owner=$(jq -r '.page_evidence_memory_missing_owner // "not_applicable"' "$aggregates")"
