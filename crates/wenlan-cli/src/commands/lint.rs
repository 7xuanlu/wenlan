// SPDX-License-Identifier: Apache-2.0
use std::collections::BTreeMap;
use std::process::ExitCode;

use wenlan_types::lint::{LintOutcome, LintReport, LintSummaryCode};

use crate::client::WenlanClient;
use crate::output::{print_json, OutputFormat};

pub async fn run(
    client: &WenlanClient,
    format: OutputFormat,
    quiet: bool,
    space: Option<String>,
) -> ExitCode {
    let report = match client.lint(space).await {
        Ok(report) => report,
        Err(error) => {
            eprintln!("wenlan lint: {error:#}");
            return ExitCode::from(2);
        }
    };
    let code = exit_code(&report);
    let rendered = match format {
        OutputFormat::Json | OutputFormat::Auto => print_json(&report),
        OutputFormat::Table if quiet => Ok(()),
        OutputFormat::Table => {
            print!("{}", render_human(&report));
            Ok(())
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
    } else if report.totals().findings() > 0 {
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
        "Lint: {} checks, {} passed, {} findings, {} incomplete\nGroups:\n",
        totals.checks(),
        totals.passed(),
        totals.findings(),
        totals.incomplete()
    );
    for (group, (checks, findings, incomplete)) in groups {
        output.push_str(&format!(
            "  {group}: {checks} check{}, {findings} findings, {incomplete} incomplete\n",
            if checks == 1 { "" } else { "s" }
        ));
    }
    append_checks(&mut output, "Findings", report, LintOutcome::Finding);
    output.push_str("Incomplete");
    let incomplete: Vec<_> = report
        .checks()
        .iter()
        .filter(|check| !matches!(check.outcome(), LintOutcome::Pass | LintOutcome::Finding))
        .collect();
    append_selected(&mut output, &incomplete);
    output
}

fn append_checks(output: &mut String, label: &str, report: &LintReport, outcome: LintOutcome) {
    output.push_str(label);
    let selected: Vec<_> = report
        .checks()
        .iter()
        .filter(|check| check.outcome() == outcome)
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
        output.push_str(&format!(
            "  {}: {}\n",
            check.check_id(),
            summary_name(check.summary_code())
        ));
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
