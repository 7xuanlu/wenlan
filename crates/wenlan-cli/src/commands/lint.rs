// SPDX-License-Identifier: Apache-2.0
use std::collections::BTreeMap;
use std::process::ExitCode;

use wenlan_types::lint::{
    LintGateEffect, LintOutcome, LintProfile, LintRecommendationCode, LintReport, LintSummaryCode,
};

use crate::client::WenlanClient;
use crate::output::{print_json, OutputFormat};

pub async fn run(
    client: &WenlanClient,
    format: OutputFormat,
    quiet: bool,
    profile: Option<LintProfile>,
    space: Option<String>,
) -> ExitCode {
    let report = match client.lint(profile, space).await {
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
    let mut groups = BTreeMap::<&str, (u32, u32, u32)>::new();
    for check in report.checks() {
        let group = check
            .check_id()
            .split_once('.')
            .map_or("other", |(group, _)| group);
        let counts = groups.entry(group).or_default();
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
    for (group, (checks, findings, incomplete)) in groups {
        output.push_str(&format!(
            "  {group}: {checks} check{}, {findings} findings, {incomplete} incomplete\n",
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
