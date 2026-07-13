#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
script="$repo_root/scripts/cleanup-legacy-captures.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
db="$tmp/fixture.db"
backup_dir="$tmp/backups"

sqlite3 "$db" <<'SQL'
CREATE TABLE memories (id TEXT PRIMARY KEY, source TEXT, source_id TEXT);
CREATE TABLE document_tags (source TEXT, source_id TEXT, tag TEXT);
CREATE TABLE capture_refs (source_id TEXT PRIMARY KEY, activity_id TEXT, source TEXT);
CREATE TABLE activities (id TEXT PRIMARY KEY);
CREATE TABLE access_log (source_id TEXT);
CREATE TABLE agent_activity (id INTEGER PRIMARY KEY, memory_ids TEXT);
CREATE TABLE memory_entities (memory_id TEXT, entity_id TEXT);
CREATE TABLE relations (source_memory_id TEXT);
CREATE TABLE eval_signals (memory_id TEXT);
CREATE TABLE eval_judgments (memory_id TEXT);
CREATE TABLE rejected_memories (similar_to_source_id TEXT);
CREATE TABLE source_sync_state (source_id TEXT);
CREATE TABLE enrichment_steps (source_id TEXT);
CREATE TABLE page_sources (memory_source_id TEXT);
CREATE TABLE summary_node_sources (memory_source_id TEXT);
CREATE TABLE document_enrichment_queue (source_id TEXT);
CREATE TABLE derived_artifact_sweeps (source_id TEXT);
CREATE TABLE pages (source_memory_ids TEXT);
CREATE TABLE page_evidence (source_kind TEXT, locator TEXT);
CREATE TABLE refinement_queue (source_ids TEXT);
CREATE VIRTUAL TABLE chunks_fts USING fts4(
    content,
    content='chunks'
);
CREATE TRIGGER memories_vector_delete AFTER DELETE ON memories
BEGIN
    SELECT vector('[1]');
END;

INSERT INTO memories VALUES ('old-1','hotkey_capture','legacy-a');
INSERT INTO memories VALUES ('old-2','hotkey_capture','legacy-a');
INSERT INTO memories VALUES ('keep-1','memory','keep-a');
INSERT INTO document_tags VALUES ('focus_capture','legacy-focus','noise');
INSERT INTO document_tags VALUES ('hotkey_capture','legacy-a','noise');
INSERT INTO document_tags VALUES ('memory','keep-a','keep');
INSERT INTO capture_refs VALUES ('legacy-a','activity-old','hotkey');
INSERT INTO capture_refs VALUES ('legacy-orphan','activity-old','hotkey');
INSERT INTO activities VALUES ('activity-old');
INSERT INTO activities VALUES ('activity-keep');
INSERT INTO access_log VALUES ('legacy-a');
INSERT INTO access_log VALUES ('keep-a');
INSERT INTO agent_activity VALUES (1,'legacy-a');
INSERT INTO agent_activity VALUES (2,'keep-a');
INSERT INTO agent_activity VALUES (3,'legacy-a-extra');
INSERT INTO memory_entities VALUES ('legacy-a','entity-old');
INSERT INTO memory_entities VALUES ('keep-a','entity-keep');
SQL

preview=$($script --db "$db")
token=$(printf '%s\n' "$preview" | sed -n 's/^plan_token=//p')
test -n "$token"
printf '%s\n' "$preview" | grep -q '^mode=dry-run$'
printf '%s\n' "$preview" | grep -q '^legacy_memory_chunks=2$'
printf '%s\n' "$preview" | grep -q '^legacy_document_tags=2$'

if $script --db "$db" --apply --expect deadbeef --backup-dir "$backup_dir"; then
    echo "wrong plan token unexpectedly applied" >&2
    exit 1
fi
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM memories WHERE source='hotkey_capture';")" = 2

sqlite3 "$db" "UPDATE document_tags SET tag='changed-after-preview' WHERE source='memory';"
if $script --db "$db" --apply --expect "$token" --backup-dir "$backup_dir"; then
    echo "stale plan token unexpectedly applied after a same-count data change" >&2
    exit 1
fi
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM memories WHERE source='hotkey_capture';")" = 2

preview=$($script --db "$db")
token=$(printf '%s\n' "$preview" | sed -n 's/^plan_token=//p')
applied=$($script --db "$db" --apply --expect "$token" --backup-dir "$backup_dir")
printf '%s\n' "$applied" | grep -q '^mode=applied$'
backup=$(printf '%s\n' "$applied" | sed -n 's/^backup=//p')
test -f "$backup"
test "$(sqlite3 "$backup" "SELECT COUNT(*) FROM memories WHERE source='hotkey_capture';")" = 2

test "$(sqlite3 "$db" "SELECT COUNT(*) FROM memories WHERE source='hotkey_capture';")" = 0
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM document_tags WHERE source IN ('focus_capture','ambient','hotkey_capture');")" = 0
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM capture_refs WHERE source='hotkey';")" = 0
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM agent_activity WHERE id=1;")" = 0
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM activities WHERE id='activity-old';")" = 0

test "$(sqlite3 "$db" "SELECT COUNT(*) FROM memories WHERE source_id='keep-a';")" = 1
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM document_tags WHERE source_id='keep-a';")" = 1
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM access_log WHERE source_id='keep-a';")" = 1
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM agent_activity WHERE id=2;")" = 1
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM agent_activity WHERE id=3;")" = 1
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM memory_entities WHERE memory_id='keep-a';")" = 1
test "$(sqlite3 "$db" "SELECT COUNT(*) FROM activities WHERE id='activity-keep';")" = 1

rerun=$($script --db "$db")
printf '%s\n' "$rerun" | grep -q '^legacy_memory_chunks=0$'
printf '%s\n' "$rerun" | grep -q '^legacy_document_tags=0$'

echo "cleanup-legacy-captures: ok"
