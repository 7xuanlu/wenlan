use super::frontmatter::{FrontmatterState, VersionValue};
use super::fs::{EntryKind, EntryScope, PageScan};
use super::path::normalize_target_path;
use super::state::{RawStateKind, StateEntryStatus};
use crate::lint::context::{LintContext, PopulationBasis, ScopeFilter};
use std::collections::{BTreeMap, BTreeSet};
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintCoverage, LintEvidenceRef,
    LintOpaqueId, LintOutcome, LintPrecondition, LintRecommendationCode, LintSeverity,
    LintSummaryCode, LintValidationMethod, LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(crate) const STATE_CONTRACT_ID: &str = "pages.projection.state_contract";
pub(crate) const IDENTITY_ID: &str = "pages.projection.identity";
pub(crate) const VERSION_ALIGNMENT_ID: &str = "pages.projection.version_alignment";

#[derive(Debug, Clone)]
struct DbPage {
    id: String,
    status: String,
    version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Pass,
    Warning,
    Error,
    Prerequisite,
}

#[derive(Debug, Default)]
struct Assessment {
    level: Option<Level>,
    population: u64,
    defects: Vec<u64>,
    inventory: bool,
}

impl Assessment {
    fn observe(&mut self, level: Level) {
        self.population = self.population.saturating_add(1);
        self.level = Some(self.level.map_or(level, |current| current.max(level)));
        if level != Level::Pass {
            self.defects.push(self.population);
        }
    }

    fn inventory(&mut self) {
        self.population = self.population.saturating_add(1);
        self.inventory = true;
        self.level.get_or_insert(Level::Pass);
    }

    fn ensure_observed(&mut self) {
        if self.population == 0 {
            self.inventory();
        }
    }

    fn result(
        mut self,
        check_id: &str,
        duration_ms: u64,
    ) -> Result<LintCheckResult, wenlan_types::lint::LintContractError> {
        self.ensure_observed();
        let level = self.level.unwrap_or(Level::Pass);
        let (outcome, severity, applicability, precondition, summary, recommendation) = match level
        {
            Level::Pass => (
                LintOutcome::Pass,
                LintSeverity::Info,
                if self.inventory {
                    LintApplicability::Inventory
                } else {
                    LintApplicability::Applicable
                },
                LintPrecondition::Ready,
                LintSummaryCode::CheckPassed,
                None,
            ),
            Level::Warning => (
                LintOutcome::Finding,
                LintSeverity::Warning,
                LintApplicability::Applicable,
                LintPrecondition::Ready,
                LintSummaryCode::FindingDetected,
                Some(LintRecommendationCode::ReviewFinding),
            ),
            Level::Error => (
                LintOutcome::Finding,
                LintSeverity::Error,
                LintApplicability::Applicable,
                LintPrecondition::Ready,
                LintSummaryCode::FindingDetected,
                Some(LintRecommendationCode::ReviewFinding),
            ),
            Level::Prerequisite => (
                LintOutcome::NotRunPrerequisite,
                LintSeverity::Error,
                LintApplicability::NotApplicable,
                LintPrecondition::MissingPrerequisite,
                LintSummaryCode::PrerequisiteUnavailable,
                Some(LintRecommendationCode::RestorePrerequisite),
            ),
        };
        let evidence = self
            .defects
            .iter()
            .take(usize::from(LINT_MAX_EVIDENCE_PER_CHECK))
            .filter_map(|ordinal| {
                usize::try_from(ordinal.saturating_sub(1))
                    .ok()
                    .and_then(LintOpaqueId::from_sorted_position)
                    .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id })
            })
            .collect::<Vec<_>>();
        LintCheckResult::try_new(LintCheckResultInput {
            check_id: check_id.to_string(),
            outcome,
            severity,
            applicability,
            precondition,
            coverage: LintCoverage::new(
                LintValidationMethod::FullEnumeration,
                self.population,
                self.population,
                LINT_MAX_EVIDENCE_PER_CHECK,
                self.defects.len() > usize::from(LINT_MAX_EVIDENCE_PER_CHECK),
                u64::try_from(evidence.len()).unwrap_or(u64::MAX),
            )?,
            metrics: Vec::new(),
            summary_code: summary,
            recommendation_code: recommendation,
            evidence,
            duration_ms,
        })
    }
}

pub(crate) async fn run(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    let pages = match load_pages(context).await {
        Ok(pages) => pages,
        Err(()) => return failed_results(context),
    };
    let selected = context.scope().filter().is_selected();
    let scan = context
        .page_scan()
        .expect("Page checks require a Page scan");
    let selected_ids = pages
        .iter()
        .map(|page| page.id.as_str())
        .collect::<BTreeSet<_>>();
    let assessments = evaluate_all(scan, &pages, selected, &selected_ids);
    assessments
        .into_iter()
        .map(|(check_id, assessment)| {
            let denominator = assessment.population;
            let basis = if selected {
                PopulationBasis::SelectedScope
            } else {
                PopulationBasis::Global
            };
            if context
                .record_population(check_id, basis, denominator)
                .is_err()
            {
                return failed_result(check_id, context.clock().duration_ms());
            }
            assessment.result(check_id, context.clock().duration_ms())
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|_| failed_results(context))
}

fn evaluate_all(
    scan: &PageScan,
    pages: &[DbPage],
    selected: bool,
    selected_ids: &BTreeSet<&str>,
) -> [(&'static str, Assessment); 3] {
    if matches!(scan.raw_state.kind, RawStateKind::FutureU32(_)) {
        return [
            (STATE_CONTRACT_ID, prerequisite_assessment()),
            (IDENTITY_ID, prerequisite_assessment()),
            (VERSION_ALIGNMENT_ID, prerequisite_assessment()),
        ];
    }
    [
        (
            STATE_CONTRACT_ID,
            evaluate_state(scan, selected.then_some(selected_ids)),
        ),
        (IDENTITY_ID, evaluate_identity(scan, pages, selected)),
        (
            VERSION_ALIGNMENT_ID,
            evaluate_versions(scan, pages, selected),
        ),
    ]
}

fn prerequisite_assessment() -> Assessment {
    let mut assessment = Assessment::default();
    assessment.observe(Level::Prerequisite);
    assessment
}

async fn load_pages(context: &LintContext<'_, '_>) -> Result<Vec<DbPage>, ()> {
    let (sql, params) = match context.scope().filter() {
        ScopeFilter::Global => (
            "SELECT id, status, version FROM pages ORDER BY id",
            libsql::params::Params::None,
        ),
        ScopeFilter::Registered(workspace) => (
            "SELECT id, status, version FROM pages WHERE workspace = ?1 ORDER BY id",
            libsql::params::Params::Positional(vec![libsql::Value::Text(workspace.clone())]),
        ),
        ScopeFilter::Uncategorized => (
            "SELECT id, status, version FROM pages WHERE workspace IS NULL ORDER BY id",
            libsql::params::Params::None,
        ),
    };
    let mut rows = context
        .snapshot()
        .query(sql, params)
        .await
        .map_err(|_| ())?;
    let mut pages = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        pages.push(DbPage {
            id: row.get(0).map_err(|_| ())?,
            status: row.get(1).map_err(|_| ())?,
            version: row.get(2).map_err(|_| ())?,
        });
    }
    Ok(pages)
}

fn evaluate_state(scan: &PageScan, selected_ids: Option<&BTreeSet<&str>>) -> Assessment {
    let mut result = Assessment::default();
    let recognized_projection = scan.page_markdown().iter().any(|entry| {
        entry.frontmatter.origin_id.as_deref().is_some_and(|id| {
            selected_ids.is_none_or(|ids| ids.contains(normalize_id(id).as_str()))
        })
    });
    match scan.raw_state.kind {
        RawStateKind::Missing if recognized_projection => result.observe(Level::Error),
        RawStateKind::Missing => result.inventory(),
        RawStateKind::WriterDefaultV0 | RawStateKind::LegacyV1 | RawStateKind::ImplicitV2 => {
            result.observe(Level::Warning)
        }
        RawStateKind::ExplicitV2 => result.observe(Level::Pass),
        RawStateKind::FutureU32(_) => result.observe(Level::Prerequisite),
        RawStateKind::NonU32 | RawStateKind::Malformed => result.observe(Level::Error),
    }
    for edge in &scan.raw_state.edges {
        if selected_ids.is_some_and(|ids| !ids.contains(normalize_id(&edge.state_id).as_str())) {
            continue;
        }
        if edge.status != StateEntryStatus::Valid {
            result.observe(Level::Error);
        } else {
            result.observe(Level::Pass);
        }
    }
    if selected_ids.is_none() {
        for _ in &scan.path_issues {
            result.observe(Level::Error);
        }
    }
    result
}

fn evaluate_identity(scan: &PageScan, pages: &[DbPage], selected: bool) -> Assessment {
    let mut result = Assessment::default();
    let active = pages
        .iter()
        .filter(|page| page.status == "active")
        .map(|page| page.id.as_str())
        .collect::<BTreeSet<_>>();
    let archived = pages
        .iter()
        .filter(|page| page.status == "archived")
        .map(|page| page.id.as_str())
        .collect::<BTreeSet<_>>();
    let db_ids = active.union(&archived).copied().collect::<BTreeSet<_>>();
    let edges = scan
        .raw_state
        .edges
        .iter()
        .filter(|edge| !selected || db_ids.contains(normalize_id(&edge.state_id).as_str()))
        .collect::<Vec<_>>();
    let state_ids = edges
        .iter()
        .map(|edge| normalize_id(&edge.state_id))
        .collect::<BTreeSet<_>>();
    let mut raw_by_normalized = BTreeMap::<String, Vec<&str>>::new();
    for edge in &edges {
        raw_by_normalized
            .entry(normalize_id(&edge.state_id))
            .or_default()
            .push(edge.state_id.as_str());
        if edge.state_id.starts_with("concept_") {
            result.observe(Level::Warning);
        }
        inspect_edge_identity(scan, edge, &mut result);
    }
    inspect_target_collisions(&edges, &mut result);
    for raw_ids in raw_by_normalized.values() {
        if raw_ids.len() > 1 {
            result.observe(Level::Warning);
        }
    }

    let mut frontmatter_counts = BTreeMap::<String, usize>::new();
    for entry in scan.page_markdown() {
        let normalized = entry.frontmatter.origin_id.as_deref().map(normalize_id);
        if selected && normalized.as_deref().is_none_or(|id| !db_ids.contains(id)) {
            continue;
        }
        match entry.frontmatter.state {
            FrontmatterState::Invalid
            | FrontmatterState::Malformed
            | FrontmatterState::Truncated
            | FrontmatterState::OverLimit => result.observe(Level::Error),
            FrontmatterState::Absent | FrontmatterState::Parsed => result.observe(Level::Pass),
            FrontmatterState::Unparsed => result.observe(Level::Error),
        }
        let Some(normalized) = normalized else {
            result.inventory();
            continue;
        };
        *frontmatter_counts.entry(normalized).or_default() += 1;
    }
    for count in frontmatter_counts.values() {
        if *count > 1 {
            result.observe(Level::Error);
        }
    }
    let frontmatter_ids = frontmatter_counts.keys().cloned().collect::<BTreeSet<_>>();
    for id in &active {
        if !state_ids.contains(*id) {
            result.observe(Level::Warning);
        }
    }
    for id in &archived {
        if !state_ids.contains(*id) {
            result.inventory();
        }
    }
    if !selected {
        for id in &state_ids {
            if !db_ids.contains(id.as_str()) {
                result.observe(Level::Warning);
            }
        }
    }
    for id in &frontmatter_ids {
        if !state_ids.contains(id) {
            result.observe(Level::Warning);
        }
    }
    result
}

fn inspect_edge_identity(
    scan: &PageScan,
    edge: &&super::state::StateEdge,
    result: &mut Assessment,
) {
    if edge.status != StateEntryStatus::Valid {
        result.observe(Level::Error);
        return;
    }
    let Some(raw_target) = edge.raw_target_path.as_deref() else {
        result.observe(Level::Error);
        return;
    };
    let Ok(target) = normalize_target_path(raw_target) else {
        result.observe(Level::Error);
        return;
    };
    let target = target.as_str();
    if target.starts_with(".wenlan/")
        || target.starts_with("_sources/")
        || !target.to_ascii_lowercase().ends_with(".md")
    {
        result.observe(Level::Error);
        return;
    }
    if target_crosses_symlink(scan, target) {
        result.observe(Level::Error);
        return;
    }
    let Some(entry) = scan.entry(target) else {
        result.observe(Level::Warning);
        return;
    };
    if entry.scope != EntryScope::PageMarkdown {
        result.observe(Level::Error);
        return;
    }
    match entry.frontmatter.origin_id.as_deref() {
        Some(origin_id) if normalize_id(origin_id) == normalize_id(&edge.state_id) => {
            result.observe(Level::Pass)
        }
        Some(_) => result.observe(Level::Error),
        None => result.inventory(),
    }
}

fn inspect_target_collisions(edges: &[&super::state::StateEdge], result: &mut Assessment) {
    let mut exact = BTreeMap::<String, String>::new();
    let mut lowercase = BTreeMap::<String, (String, String)>::new();
    for edge in edges {
        let Some(raw_target) = edge.raw_target_path.as_deref() else {
            continue;
        };
        let Ok(target) = normalize_target_path(raw_target) else {
            continue;
        };
        let target = target.as_str().to_string();
        let identity = normalize_id(&edge.state_id);
        if let Some(previous_identity) = exact.insert(target.clone(), identity.clone()) {
            if previous_identity != identity {
                result.observe(Level::Error);
            }
            continue;
        }
        let key = target.to_lowercase();
        if let Some((previous_target, previous_identity)) =
            lowercase.insert(key, (target.clone(), identity.clone()))
        {
            if previous_target != target || previous_identity != identity {
                result.observe(Level::Error);
            }
        }
    }
}

fn target_crosses_symlink(scan: &PageScan, target: &str) -> bool {
    let mut prefix = String::new();
    target.split('/').any(|component| {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        scan.entry(&prefix)
            .is_some_and(|entry| entry.kind == EntryKind::Symlink)
    })
}

fn evaluate_versions(scan: &PageScan, pages: &[DbPage], selected: bool) -> Assessment {
    let mut result = Assessment::default();
    let db_versions = pages
        .iter()
        .map(|page| (page.id.as_str(), page.version))
        .collect::<BTreeMap<_, _>>();
    for edge in &scan.raw_state.edges {
        let normalized_id = normalize_id(&edge.state_id);
        if selected && !db_versions.contains_key(normalized_id.as_str()) {
            continue;
        }
        let state_version = valid_version(edge.state_version);
        let file_version = valid_version(edge.frontmatter.origin_version);
        let db_version = db_versions.get(normalized_id.as_str()).copied();
        if state_version.is_err()
            || file_version.is_err_and(|missing| !missing)
            || db_version.is_some_and(|version| version < 0)
        {
            result.observe(Level::Error);
            continue;
        }
        let Ok(state_version) = state_version else {
            result.observe(Level::Error);
            continue;
        };
        let Ok(file_version) = file_version else {
            result.inventory();
            continue;
        };
        if file_version != state_version
            || db_version.is_some_and(|version| version != state_version)
        {
            result.observe(Level::Warning);
        } else {
            result.observe(Level::Pass);
        }
    }
    result
}

fn valid_version(value: VersionValue) -> Result<i64, bool> {
    match value {
        VersionValue::Integer(version) if version >= 0 => Ok(version),
        VersionValue::Missing => Err(true),
        VersionValue::Integer(_) | VersionValue::Invalid => Err(false),
    }
}

fn normalize_id(id: &str) -> String {
    id.strip_prefix("concept_")
        .map(|suffix| format!("page_{suffix}"))
        .unwrap_or_else(|| id.to_string())
}

fn failed_results(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    [STATE_CONTRACT_ID, IDENTITY_ID, VERSION_ALIGNMENT_ID]
        .into_iter()
        .map(|check_id| {
            let _ = context.record_population(
                check_id,
                if context.scope().filter().is_selected() {
                    PopulationBasis::SelectedScope
                } else {
                    PopulationBasis::Global
                },
                0,
            );
            failed_result(check_id, context.clock().duration_ms())
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
}

fn failed_result(
    check_id: &str,
    duration_ms: u64,
) -> Result<LintCheckResult, wenlan_types::lint::LintContractError> {
    LintCheckResult::try_new(LintCheckResultInput {
        check_id: check_id.to_string(),
        outcome: LintOutcome::FailedToRun,
        severity: LintSeverity::Error,
        applicability: LintApplicability::Applicable,
        precondition: LintPrecondition::Ready,
        coverage: LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            0,
            0,
            LINT_MAX_EVIDENCE_PER_CHECK,
            false,
            0,
        )?,
        metrics: Vec::new(),
        summary_code: LintSummaryCode::ExecutionFailed,
        recommendation_code: Some(LintRecommendationCode::InspectRuntime),
        evidence: Vec::new(),
        duration_ms,
    })
}

#[cfg(test)]
#[path = "state_checks_test.rs"]
mod tests;
