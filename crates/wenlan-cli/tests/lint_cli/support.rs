// SPDX-License-Identifier: Apache-2.0
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use wenlan_types::lint::{
    LintApplicability, LintCapabilityContext, LintCheckResult, LintCheckResultInput,
    LintConfigFingerprint, LintCoverage, LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest,
    LintEvidenceRef, LintGateEffect, LintMetric, LintMetricCode, LintMetricValue, LintOpaqueId,
    LintOutcome, LintPageSnapshotMode, LintPageSnapshotReceipt, LintPrecondition,
    LintProducerReceipt, LintReasonCode, LintRecommendationCode, LintReport, LintScope,
    LintSeverity, LintSnapshotReceipts, LintSummaryCode, LintValidationMethod,
    LINT_GENERAL_CHECK_COUNT,
};

pub fn report(checks: &[(&str, LintOutcome)]) -> LintReport {
    let checks = checks
        .iter()
        .map(|(id, outcome)| check(id, *outcome, LintGateEffect::Actionable))
        .collect();
    build_report(checks)
}

pub fn report_with_gates(checks: &[(&str, LintOutcome, LintGateEffect)]) -> LintReport {
    build_report(
        checks
            .iter()
            .map(|(id, outcome, gate_effect)| check(id, *outcome, *gate_effect))
            .collect(),
    )
}

pub fn report_with_evidence_count(check_id: &str, evidence_count: usize) -> LintReport {
    build_report(vec![check_with_evidence_count(check_id, evidence_count)])
}

fn check_with_evidence_count(id: &str, evidence_count: usize) -> LintCheckResult {
    let evidence = (0..evidence_count)
        .map(|position| LintEvidenceRef::OpaqueId {
            opaque_id: LintOpaqueId::from_sorted_position(position).unwrap(),
        })
        .collect::<Vec<_>>();
    LintCheckResult::try_new(LintCheckResultInput {
        check_id: id.to_string(),
        outcome: LintOutcome::Finding,
        severity: LintSeverity::Warning,
        applicability: LintApplicability::Applicable,
        precondition: LintPrecondition::Ready,
        coverage: LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            evidence_count as u64,
            evidence_count as u64,
            100,
            false,
            evidence_count as u64,
        )
        .expect("valid evidence fixture coverage"),
        metrics: vec![LintMetric::new(
            LintMetricCode::AffectedRecords,
            LintMetricValue::Count {
                value: evidence_count as u64,
            },
        )],
        summary_code: LintSummaryCode::FindingDetected,
        recommendation_code: Some(LintRecommendationCode::ReviewFinding),
        evidence,
        duration_ms: 1,
    })
    .expect("valid evidence fixture")
}

fn build_report(mut checks: Vec<LintCheckResult>) -> LintReport {
    let mut index = 0_usize;
    while checks.len() < LINT_GENERAL_CHECK_COUNT {
        let id = format!("fixture.padding_{index:02}");
        if !checks.iter().any(|check| check.check_id() == id) {
            checks.push(check(&id, LintOutcome::Pass, LintGateEffect::Actionable));
        }
        index += 1;
    }
    LintReport::try_new(
        LintScope::global(),
        LintCapabilityContext::daemon_operator_endpoint(),
        LintSnapshotReceipts::new(
            LintDbSnapshotReceipt::new(
                LintDbSnapshotMode::TransactionalReadOnly,
                LintDigest::from_u64(1),
                Some(LintDigest::from_u64(1)),
            ),
            LintPageSnapshotReceipt::new(
                LintPageSnapshotMode::BestEffort,
                LintDigest::from_u64(2),
                Some(LintDigest::from_u64(2)),
            ),
        ),
        LintConfigFingerprint::from_effective_config(&[]),
        LintProducerReceipt::new(None),
        checks,
    )
    .expect("valid typed lint report fixture")
}

fn check(id: &str, outcome: LintOutcome, gate_effect: LintGateEffect) -> LintCheckResult {
    let (severity, applicability, precondition, summary_code, recommendation_code) = match outcome {
        LintOutcome::Pass => (
            LintSeverity::Info,
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::CheckPassed,
            None,
        ),
        LintOutcome::Finding => (
            LintSeverity::Warning,
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::FindingDetected,
            Some(LintRecommendationCode::ReviewFinding),
        ),
        LintOutcome::NotRunPrerequisite => (
            LintSeverity::Error,
            LintApplicability::NotApplicable,
            LintPrecondition::MissingPrerequisite,
            LintSummaryCode::PrerequisiteUnavailable,
            Some(LintRecommendationCode::RestorePrerequisite),
        ),
        LintOutcome::InconsistentSnapshot => (
            LintSeverity::Error,
            LintApplicability::Applicable,
            LintPrecondition::SnapshotUnstable,
            LintSummaryCode::SnapshotInconsistent,
            Some(LintRecommendationCode::RerunAfterSnapshotStabilizes),
        ),
        LintOutcome::FailedToRun => (
            LintSeverity::Error,
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::ExecutionFailed,
            Some(LintRecommendationCode::InspectRuntime),
        ),
    };
    let (coverage, metrics, evidence) = if outcome == LintOutcome::Finding {
        (
            LintCoverage::new(LintValidationMethod::FullEnumeration, 3, 3, 100, false, 2)
                .expect("valid finding coverage"),
            vec![LintMetric::new(
                LintMetricCode::AffectedRecords,
                LintMetricValue::Count { value: 1 },
            )],
            vec![
                LintEvidenceRef::OpaqueId {
                    opaque_id: LintOpaqueId::from_sorted_position(0).unwrap(),
                },
                LintEvidenceRef::ReasonCode {
                    reason_code: LintReasonCode::InvalidCatalogState,
                },
            ],
        )
    } else {
        (
            LintCoverage::new(LintValidationMethod::FullEnumeration, 0, 0, 100, false, 0)
                .expect("valid fixture coverage"),
            Vec::new(),
            Vec::new(),
        )
    };
    LintCheckResult::try_new_with_gate_effect(
        LintCheckResultInput {
            check_id: id.to_string(),
            outcome,
            severity,
            applicability,
            precondition,
            coverage,
            metrics,
            summary_code,
            recommendation_code,
            evidence,
            duration_ms: 1,
        },
        gate_effect,
    )
    .expect("valid typed lint check fixture")
}

pub fn spawn_report(report: &LintReport) -> (String, Receiver<String>) {
    spawn_body(&serde_json::to_string(report).expect("serialize typed lint report"))
}

pub fn spawn_value(value: &Value) -> (String, Receiver<String>) {
    spawn_body(&value.to_string())
}

pub fn spawn_error(status: u16, value: &Value) -> (String, Receiver<String>) {
    spawn_status_body(status, &value.to_string())
}

pub fn spawn_oversized() -> (String, Receiver<String>) {
    spawn_response(200, "", 8 * 1024 * 1024 + 1)
}

fn spawn_body(body: &str) -> (String, Receiver<String>) {
    spawn_status_body(200, body)
}

fn spawn_status_body(status: u16, body: &str) -> (String, Receiver<String>) {
    spawn_response(status, body, body.len())
}

fn spawn_response(status: u16, body: &str, content_length: usize) -> (String, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
    let base = format!("http://{}", listener.local_addr().expect("fixture address"));
    let response_body = body.to_string();
    let (sent, received) = mpsc::channel();
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept request");
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        let _ = sent.send(request_line);
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("request header");
            if line == "\r\n" || line.is_empty() {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            content_length, response_body,
        );
        reader
            .get_mut()
            .write_all(response.as_bytes())
            .expect("write response");
    });
    (base, received)
}

pub fn closed_host() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind closed transport fixture");
    let host = format!(
        "http://{}",
        listener.local_addr().expect("closed fixture address")
    );
    drop(listener);
    host
}
