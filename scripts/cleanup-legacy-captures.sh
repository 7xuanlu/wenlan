#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: cleanup-legacy-captures.sh --db PATH [--apply --expect TOKEN --backup-dir DIR]

Default mode is read-only and prints a plan token. Applying requires the exact
token from a fresh preview, an idle database, and a backup directory.
EOF
}

db=""
apply=0
expect=""
backup_dir=""
while (($#)); do
    case "$1" in
        --db)
            db=${2:-}
            shift 2
            ;;
        --apply)
            apply=1
            shift
            ;;
        --expect)
            expect=${2:-}
            shift 2
            ;;
        --backup-dir)
            backup_dir=${2:-}
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [[ -z "$db" || ! -f "$db" ]]; then
    echo "--db must name an existing database" >&2
    exit 2
fi
if ! command -v sqlite3 >/dev/null 2>&1; then
    echo "sqlite3 is required" >&2
    exit 2
fi
if ((apply)) && [[ -z "$expect" || -z "$backup_dir" ]]; then
    echo "--apply requires --expect TOKEN and --backup-dir DIR" >&2
    exit 2
fi

query() {
    sqlite3 -batch -noheader "$db" "$1"
}

target_cte="WITH target(source_id) AS (
    SELECT DISTINCT source_id FROM memories WHERE source='hotkey_capture'
    UNION
    SELECT DISTINCT source_id FROM capture_refs WHERE source='hotkey'
)"

legacy_memory_chunks=$(query "SELECT COUNT(*) FROM memories WHERE source='hotkey_capture';")
legacy_memory_heads=$(query "SELECT COUNT(DISTINCT source_id) FROM memories WHERE source='hotkey_capture';")
legacy_document_tags=$(query "SELECT COUNT(*) FROM document_tags WHERE source IN ('focus_capture','ambient','hotkey_capture');")
legacy_capture_refs=$(query "SELECT COUNT(*) FROM capture_refs WHERE source='hotkey';")
legacy_access_rows=$(query "$target_cte SELECT COUNT(*) FROM access_log WHERE source_id IN target;")
legacy_activity_rows=$(query "$target_cte SELECT COUNT(*) FROM agent_activity a WHERE EXISTS (SELECT 1 FROM target t WHERE instr(',' || COALESCE(a.memory_ids,'') || ',', ',' || t.source_id || ',')>0);")
legacy_memory_entity_links=$(query "$target_cte SELECT COUNT(*) FROM memory_entities WHERE memory_id IN target;")
page_evidence_blockers=$(query "$target_cte SELECT COUNT(*) FROM page_evidence WHERE source_kind='memory' AND locator IN target;")
page_source_blockers=$(query "$target_cte SELECT COUNT(*) FROM page_sources WHERE memory_source_id IN target;")
page_json_blockers=$(query "$target_cte SELECT COUNT(*) FROM pages p WHERE NOT json_valid(COALESCE(p.source_memory_ids,'[]')) OR EXISTS (SELECT 1 FROM json_each(p.source_memory_ids) j JOIN target t ON j.value=t.source_id);")
refinement_blockers=$(query "$target_cte SELECT COUNT(*) FROM refinement_queue q WHERE NOT json_valid(COALESCE(q.source_ids,'[]')) OR EXISTS (SELECT 1 FROM json_each(q.source_ids) j JOIN target t ON j.value=t.source_id);")

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if ((!apply)); then
    plan_token=$(cd "$repo_root" && cargo run --quiet -p wenlan-core --example cleanup-legacy-captures -- "$db")
else
    plan_token=$expect
fi

print_plan() {
    printf '%s\n' \
        "legacy_memory_chunks=$legacy_memory_chunks" \
        "legacy_memory_heads=$legacy_memory_heads" \
        "legacy_document_tags=$legacy_document_tags" \
        "legacy_capture_refs=$legacy_capture_refs" \
        "legacy_access_rows=$legacy_access_rows" \
        "legacy_activity_rows=$legacy_activity_rows" \
        "legacy_memory_entity_links=$legacy_memory_entity_links" \
        "page_evidence_blockers=$page_evidence_blockers" \
        "page_source_blockers=$page_source_blockers" \
        "page_json_blockers=$page_json_blockers" \
        "refinement_blockers=$refinement_blockers" \
        "plan_token=$plan_token"
}

if ((!apply)); then
    echo "mode=dry-run"
    print_plan
    exit 0
fi

if [[ "$expect" != "$plan_token" ]]; then
    echo "plan token mismatch; rerun the dry-run and review the new population" >&2
    exit 3
fi
if ((page_evidence_blockers + page_source_blockers + page_json_blockers + refinement_blockers > 0)); then
    echo "cleanup blocked by Page provenance or refinement references; review them separately" >&2
    exit 3
fi
if command -v lsof >/dev/null 2>&1 && lsof -t -- "$db" >/dev/null 2>&1; then
    echo "database is in use; stop the Wenlan daemon before applying cleanup" >&2
    exit 3
fi
if [[ "$db" == *"'"* || "$backup_dir" == *"'"* ]]; then
    echo "single quotes are not supported in database or backup paths" >&2
    exit 2
fi

mkdir -p "$backup_dir"
timestamp=$(date -u +%Y%m%dT%H%M%SZ)
backup="$backup_dir/legacy-captures-$timestamp-${plan_token:0:12}.db"
sqlite3 "$db" ".backup '$backup'"
# Stock sqlite3 reports libSQL vector indexes as corrupt because it cannot load
# their extension. Prove the backup is queryable and contains the exact cleanup
# population instead of accepting that false failure.
backup_population=$(sqlite3 -batch -noheader -separator '|' "$backup" \
    "SELECT
        (SELECT COUNT(*) FROM memories WHERE source='hotkey_capture'),
        (SELECT COUNT(DISTINCT source_id) FROM memories WHERE source='hotkey_capture'),
        (SELECT COUNT(*) FROM document_tags WHERE source IN ('focus_capture','ambient','hotkey_capture')),
        (SELECT COUNT(*) FROM capture_refs WHERE source='hotkey'),
        (SELECT COUNT(*) FROM pragma_page_count);" 2>/dev/null || true)
expected_backup_population="$legacy_memory_chunks|$legacy_memory_heads|$legacy_document_tags|$legacy_capture_refs"
if [[ "$backup_population" != "$expected_backup_population|"* ]] || [[ ! -s "$backup" ]]; then
    echo "backup integrity check failed" >&2
    exit 3
fi

(cd "$repo_root" && cargo run --quiet -p wenlan-core --example cleanup-legacy-captures -- "$db" --expect "$expect")

remaining=$(query "SELECT (SELECT COUNT(*) FROM memories WHERE source='hotkey_capture') + (SELECT COUNT(*) FROM document_tags WHERE source IN ('focus_capture','ambient','hotkey_capture')) + (SELECT COUNT(*) FROM capture_refs WHERE source='hotkey');")
if [[ "$remaining" != "0" ]]; then
    echo "cleanup postcondition failed; restore from $backup" >&2
    exit 3
fi

echo "mode=applied"
echo "backup=$backup"
print_plan
