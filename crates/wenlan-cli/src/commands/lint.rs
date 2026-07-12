// SPDX-License-Identifier: Apache-2.0
use std::process::ExitCode;

use wenlan_types::lint::{
    LintCheckGroup, LintEvidenceRef, LintGateEffect, LintMetricCode, LintMetricValue, LintOutcome,
    LintProfile, LintReasonCode, LintRecommendationCode, LintReport, LintSummaryCode,
};

use crate::client::WenlanClient;
use crate::output::{print_json, OutputFormat};

pub async fn run(
    client: &WenlanClient,
    format: OutputFormat,
    quiet: bool,
    profile: Option<LintProfile>,
    space: Option<String>,
    external_egress: bool,
) -> ExitCode {
    if external_egress && profile != Some(LintProfile::Deep) {
        eprintln!("wenlan lint: --allow-external requires --profile deep");
        return ExitCode::from(2);
    }
    let report = match client.lint(profile, space, external_egress).await {
        Ok(report) => report,
        Err(error) => {
            eprintln!("wenlan lint: {error:#}");
            return ExitCode::from(2);
        }
    };
    let code = exit_code(&report);
    let rendered = if quiet {
        Ok(())
    } else {
        match format {
            OutputFormat::Json | OutputFormat::Auto => print_json(&report),
            OutputFormat::Table => {
                print!("{}", render_human(&report));
                Ok(())
            }
        }
    };
    if let Err(error) = rendered {
        eprintln!("wenlan lint: rendering report failed: {error:#}");
        return ExitCode::from(2);
    }
    ExitCode::from(code)
}

pub const fn exit_code(report: &LintReport) -> u8 {
    if !report.complete() {
        2
    } else if report.totals().actionable_findings() > 0 {
        1
    } else {
        0
    }
}

fn render_human(report: &LintReport) -> String {
    let totals = report.totals();
    let mut groups = [(0_u32, 0_u32, 0_u32); 7];
    for check in report.checks() {
        let Some(group) = LintCheckGroup::for_check_id(check.check_id()) else {
            continue;
        };
        let counts = &mut groups[group_index(group)];
        counts.0 += 1;
        match check.outcome() {
            LintOutcome::Pass => {}
            LintOutcome::Finding => counts.1 += 1,
            LintOutcome::NotRunPrerequisite
            | LintOutcome::InconsistentSnapshot
            | LintOutcome::FailedToRun => counts.2 += 1,
        }
    }
    let mut output = format!(
        "Lint: {} checks, {} passed, {} actionable findings, {} advisor{}, {} incomplete\nGroups:\n",
        totals.checks(),
        totals.passed(),
        totals.actionable_findings(),
        totals.advisory_findings(),
        if totals.advisory_findings() == 1 { "y" } else { "ies" },
        totals.incomplete()
    );
    for group in LintCheckGroup::ALL {
        let (checks, findings, incomplete) = groups[group_index(group)];
        if checks == 0 {
            continue;
        }
        output.push_str(&format!(
            "  {}: {checks} check{}, {findings} findings, {incomplete} incomplete\n",
            group.as_str(),
            if checks == 1 { "" } else { "s" }
        ));
    }
    append_findings(&mut output, "Findings", report, LintGateEffect::Actionable);
    append_findings(&mut output, "Advisories", report, LintGateEffect::Advisory);
    output.push_str("Incomplete");
    let incomplete: Vec<_> = report
        .checks()
        .iter()
        .filter(|check| !matches!(check.outcome(), LintOutcome::Pass | LintOutcome::Finding))
        .collect();
    append_selected(&mut output, &incomplete);
    output
}

fn append_findings(
    output: &mut String,
    label: &str,
    report: &LintReport,
    gate_effect: LintGateEffect,
) {
    output.push_str(label);
    let selected: Vec<_> = report
        .checks()
        .iter()
        .filter(|check| {
            check.outcome() == LintOutcome::Finding && check.gate_effect() == gate_effect
        })
        .collect();
    append_selected(output, &selected);
}

fn append_selected(output: &mut String, checks: &[&wenlan_types::lint::LintCheckResult]) {
    if checks.is_empty() {
        output.push_str(": none\n");
        return;
    }
    output.push_str(&format!(" ({}):\n", checks.len()));
    for check in checks {
        let summary = summary_name(check.summary_code());
        match check.recommendation_code() {
            Some(recommendation) => output.push_str(&format!(
                "  {}: {summary}; recommendation: {}\n",
                check.check_id(),
                recommendation_name(recommendation)
            )),
            None => output.push_str(&format!("  {}: {summary}\n", check.check_id())),
        }
        let affected = check.metrics().iter().find_map(|metric| {
            if metric.code() == LintMetricCode::AffectedRecords {
                match metric.value() {
                    LintMetricValue::Count { value } => Some(*value),
                    LintMetricValue::Boolean { .. } | LintMetricValue::CatalogCode { .. } => None,
                }
            } else {
                None
            }
        });
        let mut evidence_items = check
            .evidence()
            .iter()
            .take(8)
            .map(evidence_name)
            .collect::<Vec<_>>();
        if check.evidence().len() > evidence_items.len() {
            evidence_items.push(format!(
                "+{}_more",
                check.evidence().len() - evidence_items.len()
            ));
        }
        let evidence = evidence_items.join(",");
        output.push_str(&format!(
            "    affected={}; evaluated={}/{}; evidence={}; truncated={}\n",
            affected.map_or_else(|| "unknown".to_string(), |value| value.to_string()),
            check.coverage().evaluated(),
            check.coverage().denominator(),
            if evidence.is_empty() {
                "none"
            } else {
                &evidence
            },
            check.coverage().truncated(),
        ));
    }
}

const fn group_index(group: LintCheckGroup) -> usize {
    match group {
        LintCheckGroup::Identity => 0,
        LintCheckGroup::KnowledgeGraph => 1,
        LintCheckGroup::Memories => 2,
        LintCheckGroup::Operations => 3,
        LintCheckGroup::Pages => 4,
        LintCheckGroup::Runtime => 5,
        LintCheckGroup::Serving => 6,
    }
}

fn evidence_name(evidence: &LintEvidenceRef) -> String {
    match evidence {
        LintEvidenceRef::OpaqueId { opaque_id } => format!("opaque:{}", opaque_id.ordinal()),
        LintEvidenceRef::ReasonCode { reason_code } => {
            format!("reason:{}", reason_name(*reason_code))
        }
        LintEvidenceRef::SafeRootRelativePath {
            safe_root_relative_path,
        } => format!("path:{safe_root_relative_path:?}"),
    }
}

const fn reason_name(reason: LintReasonCode) -> &'static str {
    match reason {
        LintReasonCode::MissingArtifact => "missing_artifact",
        LintReasonCode::InvalidCatalogState => "invalid_catalog_state",
        LintReasonCode::ExpectedEmptySubstrate => "expected_empty_substrate",
        LintReasonCode::InvalidSourceConfiguration => "invalid_source_configuration",
        LintReasonCode::TerminalOperationFailure => "terminal_operation_failure",
        LintReasonCode::ExpiredRetry => "expired_retry",
        LintReasonCode::InvalidOperationState => "invalid_operation_state",
        LintReasonCode::DurableNoProgress => "durable_no_progress",
        LintReasonCode::SemanticProviderUnavailable => "semantic_provider_unavailable",
        LintReasonCode::InsufficientSemanticEvidence => "insufficient_semantic_evidence",
        LintReasonCode::SemanticExecutionFailure => "semantic_execution_failure",
    }
}

const fn recommendation_name(recommendation: LintRecommendationCode) -> &'static str {
    match recommendation {
        LintRecommendationCode::ReviewFinding => "review_finding",
        LintRecommendationCode::RestorePrerequisite => "restore_prerequisite",
        LintRecommendationCode::RerunAfterSnapshotStabilizes => "rerun_after_snapshot_stabilizes",
        LintRecommendationCode::InspectRuntime => "inspect_runtime",
    }
}

const fn summary_name(summary: LintSummaryCode) -> &'static str {
    match summary {
        LintSummaryCode::CheckPassed => "check_passed",
        LintSummaryCode::FindingDetected => "finding_detected",
        LintSummaryCode::PrerequisiteUnavailable => "prerequisite_unavailable",
        LintSummaryCode::SnapshotInconsistent => "snapshot_inconsistent",
        LintSummaryCode::ExecutionFailed => "execution_failed",
        LintSummaryCode::ExpectedEmpty => "expected_empty",
    }
}
