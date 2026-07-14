#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
probe="$repo_root/scripts/lint-maintenance-probe.sh"

fail() {
    echo "lint-maintenance-probe test failed: $*" >&2
    exit 1
}

[[ -x "$probe" ]] || fail "probe script is missing or not executable"
command -v jq >/dev/null 2>&1 || fail "jq is required"
command -v shasum >/dev/null 2>&1 || fail "shasum is required"
command -v sqlite3 >/dev/null 2>&1 || fail "sqlite3 is required"

tmp=$(mktemp -d "${TMPDIR:-/tmp}/wenlan-lint-maintenance-probe.XXXXXX")
trap 'rm -rf -- "$tmp"' EXIT

fixture_root="$tmp/fixture"
db="$fixture_root/origin_memory.db"
pages="$fixture_root/pages"
output_root="$tmp/output"
mkdir -p "$pages" "$output_root"

sqlite3 "$db" <<'SQL'
PRAGMA foreign_keys = OFF;
CREATE TABLE memories (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    source TEXT NOT NULL,
    source_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    entity_id TEXT,
    enrichment_status TEXT,
    content_hash TEXT,
    pending_revision INTEGER NOT NULL DEFAULT 0,
    supersedes TEXT,
    episode_of TEXT
);
CREATE TABLE entities (id TEXT PRIMARY KEY);
CREATE TABLE relations (id TEXT PRIMARY KEY, from_entity TEXT, to_entity TEXT);
CREATE TABLE pages (
    id TEXT PRIMARY KEY,
    entity_id TEXT,
    source_memory_ids TEXT,
    archived_at INTEGER
);
CREATE TABLE page_sources (page_id TEXT, memory_source_id TEXT);
CREATE TABLE page_evidence (page_id TEXT, source_kind TEXT, locator TEXT);
CREATE TABLE page_links (source_page_id TEXT, target_page_id TEXT);
CREATE TABLE enrichment_steps (source_id TEXT, step_name TEXT, status TEXT);
CREATE TABLE source_sync_state (source_id TEXT, file_path TEXT, content_hash TEXT);
CREATE TABLE document_enrichment_queue (source_id TEXT, file_path TEXT, status TEXT);

INSERT INTO memories VALUES
    ('row-valid', 'PRIVATE_MEMORY_BODY_CANARY', 'memory', 'logical-valid', 0, 'entity-valid', 'enriched', 'hash-valid', 0, NULL, NULL),
    ('row-valid-chunk', 'PRIVATE_SECONDARY_CHUNK_CANARY', 'memory', 'logical-valid', 1, 'entity-valid', 'enriched', 'hash-valid', 0, NULL, NULL),
    ('row-head-without-hash', 'PRIVATE_HASH_CANARY', 'memory', 'logical-no-hash', 0, NULL, 'pending', NULL, 0, NULL, NULL),
    ('revision-orphan', 'PRIVATE_REVISION_CANARY', 'memory', 'logical-revision', 0, NULL, 'enriched', NULL, 1, 'missing-revision-target', NULL),
    ('episode-valid', 'PRIVATE_EPISODE_CANARY', 'episode', 'episode-valid', 0, NULL, 'enriched', NULL, 0, NULL, 'logical-valid'),
    ('episode-orphan', 'PRIVATE_ORPHAN_EPISODE_CANARY', 'episode', 'episode-orphan', 0, NULL, 'enriched', NULL, 0, NULL, 'missing-episode-parent');
INSERT INTO entities VALUES ('entity-valid');
INSERT INTO relations VALUES ('relation-self', 'entity-valid', 'entity-valid');
INSERT INTO pages VALUES
    ('page-valid', 'entity-valid', '["logical-valid"]', NULL),
    ('page-dangling-entity', 'entity-missing', '["missing-legacy-owner"]', NULL);
INSERT INTO page_sources VALUES
    ('page-valid', 'logical-valid'),
    ('page-valid', 'row-valid'),
    ('page-valid', 'missing-source-owner');
INSERT INTO page_evidence VALUES
    ('page-valid', 'memory', 'logical-valid'),
    ('page-valid', 'memory', 'missing-evidence-owner');
INSERT INTO page_links VALUES ('page-valid', 'page-missing');
INSERT INTO enrichment_steps VALUES ('missing-enrichment-owner', 'classify', 'done');
INSERT INTO source_sync_state VALUES
    ('logical-valid', '/fixture/valid.md', 'hash-valid'),
    ('logical-no-hash', '/fixture/no-hash.md', 'hash-expected');
INSERT INTO document_enrichment_queue VALUES
    ('missing-sync-receipt', '/fixture/missing.md', 'done');
SQL

cat >"$pages/page-valid.md" <<'EOF'
---
origin_id: page-valid
---
# PRIVATE_PAGE_TITLE_CANARY

PRIVATE_PAGE_BODY_CANARY
EOF

hash_file() {
    shasum -a 256 "$1" | awk '{print $1}'
}

hash_tree() {
    local root=$1
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

db_before=$(hash_file "$db")
pages_before=$(hash_tree "$pages")

help=$($probe --help)
grep -q 'read-only' <<<"$help" || fail "help does not state read-only behavior"
grep -q 'REPO_DATA_ROOT' <<<"$help" || fail "help omits external artifact root"

stable_output=$(
    "$probe" \
        --db "$db" \
        --pages-root "$pages" \
        --output-root "$output_root"
)
stable_manifest=$(sed -n 's/^manifest=//p' <<<"$stable_output")
[[ -f "$stable_manifest" ]] || fail "stable manifest was not created"
[[ $(jq -r '.complete' "$stable_manifest") == true ]] || fail "stable run was incomplete"
[[ $(jq -r '.reason' "$stable_manifest") == stable_snapshot ]] || fail "stable reason mismatch"

stable_dir=$(dirname "$stable_manifest")
[[ $(jq -r '.source.database.before_sha256' "$stable_manifest") == "$(jq -r '.source.database.after_sha256' "$stable_manifest")" ]] \
    || fail "stable database receipts differ"
[[ $(jq -r '.source.pages.before_sha256' "$stable_manifest") == "$(jq -r '.source.pages.after_sha256' "$stable_manifest")" ]] \
    || fail "stable Page receipts differ"
[[ $(jq -r '.source.pages.before_sha256' "$stable_manifest") == "$(jq -r '.source.pages.snapshot_sha256' "$stable_manifest")" ]] \
    || fail "Page archive receipt differs from the source receipt"
[[ $(jq -r '.count_oracle.status' "$stable_manifest") == match ]] \
    || fail "stable count oracle did not match"

pages_snapshot="$stable_dir/pages-snapshot.tar"
snapshot_extract="$tmp/pages-snapshot-extract"
mkdir -p "$snapshot_extract"
tar -C "$snapshot_extract" -xf "$pages_snapshot"
[[ $(hash_tree "$snapshot_extract") == "$pages_before" ]] \
    || fail "extracted Page archive differs from the stable source receipt"

aggregates="$stable_dir/aggregates.json"
[[ -f "$aggregates" ]] || fail "aggregate output is missing"
jq -e '
    .page_sources_missing_owner == 1 and
    .page_evidence_memory_missing_owner == 1 and
    .pages_dangling_entity == 1 and
    .pending_revision_missing_target == 1 and
    .enrichment_steps_missing_owner == 1 and
    .legacy_page_sources_missing_owner == 1 and
    .relation_self_edges == 1 and
    .episodes_missing_parent == 1 and
    .broken_nonnull_page_links == 1 and
    .done_queue_missing_sync_receipt == 1 and
    .multi_chunk_memory_sources == 1 and
    .content_hash_missing_heads == 1
' "$aggregates" >/dev/null || fail "aggregate counts do not match the fixture"

if grep -E 'PRIVATE_(MEMORY|SECONDARY|HASH|EPISODE|PAGE)' "$aggregates" >/dev/null; then
    fail "aggregate output leaked fixture content"
fi
if grep -E 'PRIVATE_(MEMORY|SECONDARY|HASH|EPISODE|PAGE)' <<<"$stable_output" >/dev/null; then
    fail "stdout leaked fixture content"
fi

dir_mode=$(stat -f '%Lp' "$stable_dir" 2>/dev/null || stat -c '%a' "$stable_dir")
[[ "$dir_mode" == 700 ]] || fail "run directory mode is $dir_mode, expected 700"
while IFS= read -r artifact; do
    artifact_mode=$(stat -f '%Lp' "$stable_dir/$artifact" 2>/dev/null || stat -c '%a' "$stable_dir/$artifact")
    [[ "$artifact_mode" == 600 ]] || fail "$artifact mode is $artifact_mode, expected 600"
done < <(jq -r '.artifacts[].path' "$stable_manifest")

[[ $(hash_file "$db") == "$db_before" ]] || fail "stable run mutated the source database"
[[ $(hash_tree "$pages") == "$pages_before" ]] || fail "stable run mutated the Page tree"

legacy_db="$tmp/legacy-schema.db"
sqlite3 "$legacy_db" 'CREATE TABLE memories (id TEXT PRIMARY KEY);'
legacy_output=$(
    "$probe" \
        --db "$legacy_db" \
        --pages-root "$pages" \
        --output-root "$output_root"
)
legacy_manifest=$(sed -n 's/^manifest=//p' <<<"$legacy_output")
[[ $(jq -r '.complete' "$legacy_manifest") == false ]] \
    || fail "missing schema was marked complete"
[[ $(jq -r '.reason' "$legacy_manifest") == schema_unsupported ]] \
    || fail "missing schema reason mismatch"
[[ $(jq -r '.schema.aggregate_status' "$legacy_manifest") == not_applicable ]] \
    || fail "missing schema fabricated aggregate results"

hook="$tmp/mutate-page-after-first-receipt.sh"
cat >"$hook" <<EOF
#!/usr/bin/env bash
printf '%s\n' 'drift' >>'$pages/page-valid.md'
EOF
chmod 700 "$hook"

drift_output=$(
    "$probe" \
        --db "$db" \
        --pages-root "$pages" \
        --output-root "$output_root" \
        --test-after-first-receipt-hook "$hook"
)
drift_manifest=$(sed -n 's/^manifest=//p' <<<"$drift_output")
[[ -f "$drift_manifest" ]] || fail "drift manifest was not created"
[[ $(jq -r '.complete' "$drift_manifest") == false ]] || fail "drift run was marked complete"
[[ $(jq -r '.reason' "$drift_manifest") == inconsistent_snapshot ]] || fail "drift reason mismatch"

assert_rejected() {
    local label=$1
    shift
    if "$probe" "$@" >"$tmp/$label.out" 2>"$tmp/$label.err"; then
        fail "$label was unexpectedly accepted"
    fi
    grep -qi 'symlink' "$tmp/$label.err" || fail "$label did not explain symlink rejection"
}

ln -s "$db" "$tmp/db-link"
assert_rejected db-symlink \
    --db "$tmp/db-link" --pages-root "$pages" --output-root "$output_root"

ln -s "$pages" "$tmp/pages-link"
assert_rejected pages-symlink \
    --db "$db" --pages-root "$tmp/pages-link" --output-root "$output_root"

mkdir -p "$tmp/real-output"
ln -s "$tmp/real-output" "$tmp/output-link"
before_output_entries=$(find "$tmp/real-output" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')
assert_rejected output-symlink \
    --db "$db" --pages-root "$pages" --output-root "$tmp/output-link"
after_output_entries=$(find "$tmp/real-output" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')
[[ "$before_output_entries" == "$after_output_entries" ]] || fail "rejected output symlink created artifacts"

worktree_output="$repo_root/target/lint-probe-rejected-output-$RANDOM"
if "$probe" --db "$db" --pages-root "$pages" --output-root "$worktree_output" \
    >"$tmp/worktree-output.out" 2>"$tmp/worktree-output.err"; then
    fail "worktree output root was unexpectedly accepted"
fi
grep -qi 'outside git worktrees' "$tmp/worktree-output.err" \
    || fail "worktree output rejection was not explained"
[[ ! -e "$worktree_output" ]] || fail "rejected worktree output path was created"

echo "lint-maintenance-probe: ok"
