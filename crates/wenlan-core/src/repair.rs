// SPDX-License-Identifier: Apache-2.0
//! Durable preparation and CAS application of approval-gated repairs.

use crate::{
    db::MemoryDB,
    error::WenlanError,
    lint::snapshot::{LintReadSnapshot, SnapshotError, SnapshotReceipt},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;
use wenlan_types::{
    lint::{LintDigest, LintEvidenceRef, LintScope, LintScopeKind, LintSemanticAction},
    repair::{
        PrepareRepairRequest, RepairAllowedEffects, RepairDigest, RepairExpectedState,
        RepairLintScope, RepairManifest, RepairManifestDraft, RepairMutation, RepairPostAssertions,
        RepairRollbackArtifact, RepairScope, RepairSource, RepairTarget, RepairWriter,
        REPAIR_ROLLBACK_FORMAT_VERSION,
    },
};

#[cfg(unix)]
use std::fs::File;

const MANIFEST_FILE: &str = "manifest.json";
const ROLLBACK_FILE: &str = "rollback-v1.json";

#[derive(Debug, Clone)]
pub struct RepairArtifactStore {
    root: PathBuf,
}

impl RepairArtifactStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_dir(&self, manifest_id: &str) -> Result<PathBuf, WenlanError> {
        if !safe_manifest_id(manifest_id) {
            return Err(WenlanError::Validation(
                "invalid_repair_manifest_id".to_string(),
            ));
        }
        Ok(self.root.join(manifest_id))
    }

    pub fn load_manifest(&self, manifest_id: &str) -> Result<RepairManifest, WenlanError> {
        let path = self.manifest_dir(manifest_id)?.join(MANIFEST_FILE);
        let manifest: RepairManifest = serde_json::from_slice(&fs::read(path)?)?;
        if manifest.manifest_id() != manifest_id {
            return Err(WenlanError::Validation(
                "repair_manifest_id_mismatch".to_string(),
            ));
        }
        let actual = repair_digest(&manifest.canonical_unsigned_bytes()?);
        if &actual != manifest.manifest_digest() {
            return Err(WenlanError::Validation(
                "repair_manifest_digest_mismatch".to_string(),
            ));
        }
        Ok(manifest)
    }

    fn persist_prepared(
        &self,
        manifest: &RepairManifest,
        rollback_bytes: &[u8],
    ) -> Result<(), WenlanError> {
        ensure_private_dir(&self.root)?;
        let final_dir = self.manifest_dir(manifest.manifest_id())?;
        if final_dir.exists() {
            return Err(WenlanError::Conflict("repair_manifest_exists".to_string()));
        }
        let temp_dir = self.root.join(format!(
            ".{}.tmp-{}",
            manifest.manifest_id(),
            Uuid::new_v4()
        ));
        fs::create_dir(&temp_dir)?;
        set_private_dir_permissions(&temp_dir)?;
        let result = (|| {
            write_private_file(&temp_dir.join(ROLLBACK_FILE), rollback_bytes)?;
            write_private_file(
                &temp_dir.join(MANIFEST_FILE),
                &serde_json::to_vec_pretty(manifest)?,
            )?;
            sync_dir(&temp_dir)?;
            fs::rename(&temp_dir, &final_dir)?;
            sync_dir(&self.root)?;
            Ok::<(), WenlanError>(())
        })();
        if result.is_err() && temp_dir.exists() {
            let _ = fs::remove_dir_all(&temp_dir);
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRollbackArtifact {
    format_version: u16,
    table: String,
    source_id: String,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

struct ResolvedTarget {
    source_id: String,
    memory_type: Option<String>,
    space: Option<String>,
    version: Option<i64>,
    evidence_id: LintDigest,
}

pub fn semantic_record_digest(kind: &str, durable_id: &str) -> LintDigest {
    crate::lint::semantic_record_digest(kind, durable_id)
}

pub async fn prepare_memory_reclassification(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    request: PrepareRepairRequest,
    now_epoch: i64,
) -> Result<RepairManifest, WenlanError> {
    if now_epoch <= 0 {
        return Err(WenlanError::Validation(
            "invalid_repair_prepared_at".to_string(),
        ));
    }
    validate_selected_finding(&request)?;

    let snapshot = db.open_lint_snapshot().await.map_err(snapshot_error)?;
    validate_durable_scope(
        &snapshot,
        request.lint_scope(),
        request.deep_report().scope(),
    )
    .await?;
    let target = resolve_target(&snapshot, &request).await?;
    let rollback = capture_rollback(&snapshot, &target.source_id).await?;
    let rollback_bytes = serde_json::to_vec_pretty(&rollback)?;
    let target_receipt = target_receipt(&rollback)?;
    let snapshot_receipt = snapshot.finish().await.map_err(snapshot_error)?;
    validate_source_receipts(&request, snapshot_receipt)?;

    let after_memory_type = request.after_memory_type().to_string();
    let mutation =
        RepairMutation::try_reclassify(target.memory_type.as_deref(), after_memory_type.as_str())
            .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let target_scope = match target.space {
        Some(space) => RepairScope::registered(space),
        None => Ok(RepairScope::uncategorized()),
    }
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let repair_target = RepairTarget::memory(target.source_id, target_scope)
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let general = request.general_report();
    let deep = request.deep_report();
    let agent_work_digest = deep
        .agent_work()
        .ok_or_else(|| WenlanError::Validation("repair_agent_work_missing".to_string()))?
        .work_digest()
        .clone();
    let source = RepairSource::try_new(
        request.lint_scope().clone(),
        deep.scope().clone(),
        request.selected_finding().clone(),
        general.snapshots().clone(),
        deep.snapshots().clone(),
        general.producer_receipt().clone(),
        deep.producer_receipt().clone(),
        agent_work_digest,
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let allowed_effects = RepairAllowedEffects::memory_type(repair_target.clone());
    let rollback_contract =
        RepairRollbackArtifact::try_new(ROLLBACK_FILE.to_string(), repair_digest(&rollback_bytes))
            .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let post_assertions = RepairPostAssertions::try_new(target.evidence_id, vec![])
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let manifest_id = format!("repair_{}", Uuid::new_v4());
    let draft = RepairManifestDraft::try_new(
        manifest_id,
        now_epoch,
        source,
        repair_target,
        RepairExpectedState::try_new(target.version, target_receipt)
            .map_err(|error| WenlanError::Validation(error.to_string()))?,
        RepairWriter::ReclassifyMemory,
        mutation,
        allowed_effects,
        rollback_contract,
        post_assertions,
    )
    .map_err(|error| WenlanError::Validation(error.to_string()))?;
    let manifest_digest = repair_digest(&draft.canonical_bytes()?);
    let manifest = RepairManifest::try_new(draft, manifest_digest)
        .map_err(|error| WenlanError::Validation(error.to_string()))?;
    store.persist_prepared(&manifest, &rollback_bytes)?;
    Ok(manifest)
}

fn validate_selected_finding(request: &PrepareRepairRequest) -> Result<(), WenlanError> {
    if request.selected_finding().proposed_action() != LintSemanticAction::ReclassifyMemory
        || request.selected_finding().evidence_ids().len() != 1
        || !request.selected_finding().counterevidence_ids().is_empty()
    {
        return Err(WenlanError::Validation(
            "unsupported_repair_finding".to_string(),
        ));
    }
    let present = request.deep_report().checks().iter().any(|check| {
        check.check_id() == wenlan_types::repair::REPAIR_CLASSIFICATION_CHECK_ID
            && check.evidence().iter().any(|evidence| {
                matches!(evidence, LintEvidenceRef::SemanticFinding { finding } if finding == request.selected_finding())
            })
    });
    if !present {
        return Err(WenlanError::Validation(
            "repair_finding_not_in_deep_report".to_string(),
        ));
    }
    Ok(())
}

async fn validate_durable_scope(
    snapshot: &LintReadSnapshot<'_>,
    lint_scope: &RepairLintScope,
    report_scope: &LintScope,
) -> Result<(), WenlanError> {
    if !lint_scope.matches_report_scope_kind(report_scope) {
        return Err(WenlanError::Validation("repair_scope_mismatch".to_string()));
    }
    let RepairLintScope::Registered { space } = lint_scope else {
        return Ok(());
    };
    let mut rows = snapshot
        .query(
            "SELECT (SELECT COUNT(*) FROM spaces prior WHERE prior.name < current.name)
             FROM spaces current WHERE current.name=?1 LIMIT 1",
            libsql::params::Params::Positional(vec![libsql::Value::Text(space.clone())]),
        )
        .await
        .map_err(snapshot_error)?;
    let Some(row) = rows.next().await.map_err(snapshot_error)? else {
        return Err(WenlanError::Validation(
            "repair_scope_not_registered".to_string(),
        ));
    };
    let position = usize::try_from(row.get::<i64>(0).map_err(database_error)?)
        .map_err(|_| WenlanError::Validation("repair_scope_mismatch".to_string()))?;
    let expected = wenlan_types::lint::LintOpaqueId::from_sorted_position(position)
        .ok_or_else(|| WenlanError::Validation("repair_scope_mismatch".to_string()))?;
    if report_scope.kind() != LintScopeKind::Registered
        || report_scope.opaque_scope_ref() != Some(expected)
    {
        return Err(WenlanError::Validation("repair_scope_mismatch".to_string()));
    }
    Ok(())
}

async fn resolve_target(
    snapshot: &LintReadSnapshot<'_>,
    request: &PrepareRepairRequest,
) -> Result<ResolvedTarget, WenlanError> {
    let (scope_clause, params) = match request.lint_scope() {
        RepairLintScope::Global => (String::new(), libsql::params::Params::None),
        RepairLintScope::Registered { space } => (
            " AND m.space=?1".to_string(),
            libsql::params::Params::Positional(vec![libsql::Value::Text(space.clone())]),
        ),
        RepairLintScope::Uncategorized => (
            " AND m.space IS NULL".to_string(),
            libsql::params::Params::None,
        ),
    };
    let sql = format!(
        "SELECT m.source_id,m.memory_type,m.space,m.version
         FROM memories m
         WHERE m.source='memory' AND m.chunk_index=0 AND m.pending_revision=0
           AND COALESCE(m.is_recap,0)=0 AND m.supersede_mode!='evicted'{scope_clause}
         ORDER BY m.source_id,m.id"
    );
    let target_evidence = request.selected_finding().evidence_ids()[0].clone();
    let mut rows = snapshot.query(&sql, params).await.map_err(snapshot_error)?;
    let mut matches = Vec::new();
    while let Some(row) = rows.next().await.map_err(snapshot_error)? {
        let source_id: String = row.get(0).map_err(database_error)?;
        if semantic_record_digest("memory", &source_id) == target_evidence {
            matches.push(ResolvedTarget {
                source_id,
                memory_type: row.get(1).map_err(database_error)?,
                space: row.get(2).map_err(database_error)?,
                version: row.get(3).map_err(database_error)?,
                evidence_id: target_evidence.clone(),
            });
        }
    }
    if matches.len() != 1 {
        return Err(WenlanError::Validation(
            "repair_target_not_unique".to_string(),
        ));
    }
    Ok(matches.remove(0))
}

async fn capture_rollback(
    snapshot: &LintReadSnapshot<'_>,
    source_id: &str,
) -> Result<StoredRollbackArtifact, WenlanError> {
    let mut column_rows = snapshot
        .query("PRAGMA table_info(memories)", libsql::params::Params::None)
        .await
        .map_err(snapshot_error)?;
    let mut columns = Vec::new();
    while let Some(row) = column_rows.next().await.map_err(snapshot_error)? {
        columns.push(row.get::<String>(1).map_err(database_error)?);
    }
    drop(column_rows);
    if columns.is_empty() {
        return Err(WenlanError::Validation(
            "repair_target_schema_missing".to_string(),
        ));
    }
    let selected = columns
        .iter()
        .map(|column| format!("quote({})", quote_identifier(column)))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT {selected} FROM memories
         WHERE source='memory' AND source_id=?1 ORDER BY chunk_index,id"
    );
    let mut query_rows = snapshot
        .query(
            &sql,
            libsql::params::Params::Positional(vec![libsql::Value::Text(source_id.to_string())]),
        )
        .await
        .map_err(snapshot_error)?;
    let mut rows = Vec::new();
    while let Some(row) = query_rows.next().await.map_err(snapshot_error)? {
        let mut values = Vec::with_capacity(columns.len());
        for index in 0..columns.len() {
            let index = i32::try_from(index).map_err(|_| {
                WenlanError::Validation("repair_target_schema_too_wide".to_string())
            })?;
            values.push(row.get::<String>(index).map_err(database_error)?);
        }
        rows.push(values);
    }
    if rows.is_empty() {
        return Err(WenlanError::NotFound("repair_target_missing".to_string()));
    }
    Ok(StoredRollbackArtifact {
        format_version: REPAIR_ROLLBACK_FORMAT_VERSION,
        table: "memories".to_string(),
        source_id: source_id.to_string(),
        columns,
        rows,
    })
}

fn target_receipt(rollback: &StoredRollbackArtifact) -> Result<RepairDigest, WenlanError> {
    let mut bytes = b"wenlan-repair-target-v1".to_vec();
    bytes.extend(serde_json::to_vec(rollback)?);
    Ok(repair_digest(&bytes))
}

fn validate_source_receipts(
    request: &PrepareRepairRequest,
    current: SnapshotReceipt,
) -> Result<(), WenlanError> {
    if !current.is_consistent() {
        return Err(WenlanError::Conflict(
            "repair_snapshot_inconsistent".to_string(),
        ));
    }
    let current = lint_digest(current.analysis_receipt_digest().as_bytes());
    for report in [request.general_report(), request.deep_report()] {
        let db = report.snapshots().db();
        if db.analysis_digest() != &current || db.post_run_digest() != Some(&current) {
            return Err(WenlanError::Conflict(
                "repair_source_reports_stale".to_string(),
            ));
        }
    }
    Ok(())
}

fn lint_digest(bytes: [u8; 32]) -> LintDigest {
    LintDigest::from_u64(u64::from_le_bytes(
        bytes[..8].try_into().expect("eight bytes"),
    ))
}

fn repair_digest(bytes: &[u8]) -> RepairDigest {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("write to string");
    }
    RepairDigest::parse(&encoded).expect("sha256 is lowercase hex")
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn safe_manifest_id(value: &str) -> bool {
    value.starts_with("repair_")
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && Path::new(value).components().count() == 1
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), WenlanError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    set_private_file_permissions(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn ensure_private_dir(path: &Path) -> Result<(), WenlanError> {
    fs::create_dir_all(path)?;
    set_private_dir_permissions(path)
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<(), WenlanError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<(), WenlanError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), WenlanError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), WenlanError> {
    Ok(())
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<(), WenlanError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<(), WenlanError> {
    Ok(())
}

fn snapshot_error(error: SnapshotError) -> WenlanError {
    WenlanError::VectorDb(format!("repair snapshot: {error}"))
}

fn database_error(error: libsql::Error) -> WenlanError {
    WenlanError::VectorDb(format!("repair database: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{tests::test_db, MemoryDB},
        lint::{
            context::{CancellationToken, LintClock},
            runner::LintRunner,
            snapshot::LintReadSnapshot,
        },
    };
    use wenlan_types::{
        lint::{
            LintAgentSubmission, LintAgentVerdict, LintEvidenceRef, LintProfile, LintQuery,
            LintSemanticCheckId, LintSemanticDecision,
        },
        repair::{PrepareRepairRequest, RepairLintScope},
        MemoryType,
    };

    async fn fixture() -> (MemoryDB, tempfile::TempDir) {
        let (db, dir) = test_db().await;
        db.conn
            .lock()
            .await
            .execute_batch(
                "INSERT INTO spaces (id,name,created_at,updated_at)
                 VALUES ('space-personal','personal',1,1),('space-work','work',1,1);
                 INSERT INTO memories
                     (id,content,source,source_id,title,chunk_index,last_modified,chunk_type,
                      pending_revision,is_recap,supersede_mode,memory_type,space)
                 VALUES ('row-target','Target decision','memory','mem_target','target',0,10,
                         'text',0,0,'hide',NULL,'work'),
                        ('row-other','Other fact','memory','mem_other','other',0,11,
                         'text',0,0,'hide','fact','personal');",
            )
            .await
            .unwrap();
        (db, dir)
    }

    async fn request(db: &MemoryDB) -> PrepareRepairRequest {
        request_for_scope(db, RepairLintScope::global(), None).await
    }

    async fn request_for_scope(
        db: &MemoryDB,
        lint_scope: RepairLintScope,
        space: Option<&str>,
    ) -> PrepareRepairRequest {
        let general = LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .run(
                db,
                &LintQuery::new(Some(LintProfile::General), space.map(str::to_string)),
                None,
                false,
            )
            .await
            .unwrap();
        let prepared = LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .with_semantic_agent_assist()
            .run(
                db,
                &LintQuery::new(Some(LintProfile::Deep), space.map(str::to_string)),
                None,
                false,
            )
            .await
            .unwrap();
        let work = prepared.agent_work().expect("agent work");
        let verdicts = work
            .candidates()
            .iter()
            .map(|candidate| {
                let finding = candidate.check_id() == LintSemanticCheckId::MemoryClassification;
                LintAgentVerdict::try_new(
                    candidate.reference(),
                    if finding {
                        LintSemanticDecision::Finding
                    } else {
                        LintSemanticDecision::Pass
                    },
                    None,
                    candidate.reason_code(),
                    9_000,
                    vec![],
                )
                .unwrap()
            })
            .collect();
        let submission =
            LintAgentSubmission::try_new(work.work_digest().clone(), verdicts).unwrap();
        let deep = LintRunner::new(LintClock::fixed(), CancellationToken::new())
            .with_semantic_agent_submission(submission)
            .run(
                db,
                &LintQuery::new(Some(LintProfile::Deep), space.map(str::to_string)),
                None,
                false,
            )
            .await
            .unwrap();
        let deep = complete_deep_fixture(deep);
        let finding = deep
            .checks()
            .iter()
            .find(|check| check.check_id() == LintSemanticCheckId::MemoryClassification.as_str())
            .and_then(|check| {
                check.evidence().iter().find_map(|evidence| match evidence {
                    LintEvidenceRef::SemanticFinding { finding } => Some(finding.clone()),
                    _ => None,
                })
            })
            .expect("classification finding");
        assert!(general.complete(), "general totals: {:?}", general.totals());
        assert!(
            deep.complete(),
            "deep totals: {:?}; incomplete: {:?}",
            deep.totals(),
            deep.checks()
                .iter()
                .filter(|check| !matches!(
                    check.outcome(),
                    wenlan_types::lint::LintOutcome::Pass
                        | wenlan_types::lint::LintOutcome::Finding
                ))
                .map(|check| (check.check_id(), check.outcome()))
                .collect::<Vec<_>>()
        );
        assert!(deep.agent_work().is_some(), "final Deep lost agent work");
        PrepareRepairRequest::try_new(lint_scope, general, deep, finding, MemoryType::Decision)
            .unwrap()
    }

    fn complete_deep_fixture(
        report: wenlan_types::lint::LintReport,
    ) -> wenlan_types::lint::LintReport {
        let mut value = serde_json::to_value(report).unwrap();
        let checks = value["checks"].as_array_mut().unwrap();
        for check in checks.iter_mut() {
            let complete = matches!(check["outcome"].as_str(), Some("pass" | "finding"));
            if !complete {
                check["outcome"] = serde_json::json!("pass");
                check["severity"] = serde_json::json!("info");
                check["applicability"] = serde_json::json!("expected_empty");
                check["precondition"] = serde_json::json!("expected_empty");
            }
        }
        let passed = checks
            .iter()
            .filter(|check| check["outcome"] == "pass")
            .count();
        let findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding")
            .count();
        let actionable_findings = checks
            .iter()
            .filter(|check| check["outcome"] == "finding" && check["gate_effect"] == "actionable")
            .count();
        value["totals"] = serde_json::json!({
            "checks": checks.len(),
            "passed": passed,
            "findings": findings,
            "actionable_findings": actionable_findings,
            "advisory_findings": findings - actionable_findings,
            "incomplete": 0
        });
        value["complete"] = serde_json::json!(true);
        serde_json::from_value(value).unwrap()
    }

    async fn fingerprint(db: &MemoryDB) -> [u8; 32] {
        let snapshot = LintReadSnapshot::open(&db._db).await.unwrap();
        let fingerprint = snapshot.analysis_digest().unwrap().as_bytes();
        snapshot.finish().await.unwrap();
        fingerprint
    }

    #[tokio::test]
    async fn prepare_resolves_one_hashed_memory_without_mutating_store() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let request = request(&db).await;
        let before = fingerprint(&db).await;

        let manifest = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            request,
            1_721_000_000,
        )
        .await
        .unwrap();

        assert_eq!(manifest.target().memory_source_id(), "mem_target");
        assert_eq!(before, fingerprint(&db).await);
        let manifest_dir = repair_root.path().join(manifest.manifest_id());
        assert!(manifest_dir.join("manifest.json").is_file());
        assert!(manifest_dir.join("rollback-v1.json").is_file());
        assert_eq!(
            RepairArtifactStore::new(repair_root.path().to_path_buf())
                .load_manifest(manifest.manifest_id())
                .unwrap(),
            manifest
        );
    }

    #[tokio::test]
    async fn prepare_rejects_stale_lint_receipt_without_writing_artifacts() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let request = request(&db).await;
        db.conn
            .lock()
            .await
            .execute(
                "UPDATE memories SET title='changed' WHERE source_id='mem_other'",
                (),
            )
            .await
            .unwrap();

        let result = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            request,
            1_721_000_000,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn prepare_rejects_registered_label_bound_to_another_opaque_scope() {
        let (db, _db_dir) = fixture().await;
        let repair_root = tempfile::tempdir().unwrap();
        let request = request_for_scope(
            &db,
            RepairLintScope::registered("work".to_string()).unwrap(),
            Some("work"),
        )
        .await;
        let mut value = serde_json::to_value(request).unwrap();
        value["lint_scope"]["space"] = serde_json::json!("personal");
        let rebound: PrepareRepairRequest = serde_json::from_value(value).unwrap();

        let result = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            rebound,
            1_721_000_000,
        )
        .await;

        assert!(matches!(
            result,
            Err(WenlanError::Validation(message)) if message == "repair_scope_mismatch"
        ));
        assert_eq!(std::fs::read_dir(repair_root.path()).unwrap().count(), 0);
    }

    #[test]
    fn semantic_digest_binds_kind_and_durable_id() {
        assert_eq!(
            semantic_record_digest("memory", "mem_target"),
            semantic_record_digest("memory", "mem_target")
        );
        assert_ne!(
            semantic_record_digest("memory", "mem_target"),
            semantic_record_digest("entity", "mem_target")
        );
    }
}
