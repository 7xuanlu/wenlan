// SPDX-License-Identifier: Apache-2.0
//! Wire types for `GET /api/activity` — one call describing all background
//! organizing work, grouped by asset (Memories, Entities, Pages).
//!
//! Counts, enum labels and model ids only: never page or memory prose, so the
//! route is truth-manifest `NotApplicable` like `/api/config/routing`.

use serde::{Deserialize, Serialize};

/// Overall background-work state. Precedence: Blocked over Organizing over
/// UpToDate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    UpToDate,
    Organizing,
    Blocked,
}

/// The three asset groups the Activity surface reports on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityAssetKind {
    Memories,
    Entities,
    Pages,
}

/// One background step inside an asset. `Store` and `Confirm` finish without
/// a model lane (the import request stores; the user confirms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStepName {
    Store,
    Summarize,
    Link,
    Detect,
    Confirm,
    Write,
}

impl ActivityStepName {
    /// The job whose resolved lane serves this step; `None` for the laneness
    /// steps `Store` (done inside the import request) and `Confirm`
    /// (user-driven in the Wiki).
    pub fn job(self) -> Option<ActivityJob> {
        match self {
            ActivityStepName::Summarize | ActivityStepName::Link | ActivityStepName::Detect => {
                Some(ActivityJob::Everyday)
            }
            ActivityStepName::Write => Some(ActivityJob::Synthesis),
            ActivityStepName::Store | ActivityStepName::Confirm => None,
        }
    }
}

/// Per-step liveness. There is no queued/pending variant: anything not done
/// and not failed is in flight while its lane is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStepState {
    Idle,
    Running,
    Blocked,
}

/// The two background job classes, served by the resolved routing lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityJob {
    Everyday,
    Synthesis,
}

/// Resolved lane serving a job. Mirrors `JobRoute.source` strings verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityLane {
    OnDevice,
    External,
    Anthropic,
    Basic,
    None,
}

/// One resolved job route: which lane serves the job and whether it can run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityRoute {
    pub job: ActivityJob,
    pub lane: ActivityLane,
    pub model: Option<String>,
    /// "pinned" | "pinned_unavailable" | "unconfigured", verbatim from routing.
    pub mode: String,
    /// True only when `mode == "pinned"` and `model.is_some()`.
    pub available: bool,
}

/// Progress of one background step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityStep {
    pub name: ActivityStepName,
    pub state: ActivityStepState,
    pub done: u64,
    pub total: u64,
    pub failed: u64,
    /// The job whose lane serves this step; `None` for Store and Confirm.
    pub job: Option<ActivityJob>,
}

/// Progress of one asset group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityAssetStatus {
    pub kind: ActivityAssetKind,
    pub state: ActivityStepState,
    pub done: u64,
    pub total: u64,
    /// Items that cannot proceed: failed steps, or pending steps with no lane.
    pub blocked: u64,
    pub steps: Vec<ActivityStep>,
}

/// The `GET /api/activity` response body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityResponse {
    pub state: ActivityState,
    /// Unix seconds of the newest enrichment step write; None on an empty DB.
    pub last_activity_at: Option<i64>,
    /// Always three entries in order Memories, Entities, Pages.
    pub assets: Vec<ActivityAssetStatus>,
    pub everyday: ActivityRoute,
    pub synthesis: ActivityRoute,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_wire_labels_are_snake_case() {
        assert_eq!(
            serde_json::to_value(ActivityState::UpToDate).unwrap(),
            serde_json::Value::String("up_to_date".to_string())
        );
        assert_eq!(
            serde_json::to_value(ActivityLane::OnDevice).unwrap(),
            serde_json::Value::String("on_device".to_string())
        );
        for (state, label) in [
            (ActivityState::UpToDate, "up_to_date"),
            (ActivityState::Organizing, "organizing"),
            (ActivityState::Blocked, "blocked"),
        ] {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, format!("\"{label}\""));
        }
    }

    #[test]
    fn activity_step_job_mapping() {
        assert_eq!(
            ActivityStepName::Summarize.job(),
            Some(ActivityJob::Everyday)
        );
        assert_eq!(ActivityStepName::Link.job(), Some(ActivityJob::Everyday));
        assert_eq!(ActivityStepName::Detect.job(), Some(ActivityJob::Everyday));
        assert_eq!(ActivityStepName::Write.job(), Some(ActivityJob::Synthesis));
        assert_eq!(ActivityStepName::Store.job(), None);
        assert_eq!(ActivityStepName::Confirm.job(), None);
    }

    #[test]
    fn activity_response_round_trips_with_mode_passthrough() {
        let response = ActivityResponse {
            state: ActivityState::Blocked,
            last_activity_at: Some(1_700_000_000),
            assets: vec![ActivityAssetStatus {
                kind: ActivityAssetKind::Pages,
                state: ActivityStepState::Blocked,
                done: 12,
                total: 15,
                blocked: 3,
                steps: vec![ActivityStep {
                    name: ActivityStepName::Write,
                    state: ActivityStepState::Blocked,
                    done: 12,
                    total: 15,
                    failed: 0,
                    job: Some(ActivityJob::Synthesis),
                }],
            }],
            everyday: ActivityRoute {
                job: ActivityJob::Everyday,
                lane: ActivityLane::OnDevice,
                model: Some("qwen3-4b".to_string()),
                mode: "pinned".to_string(),
                available: true,
            },
            synthesis: ActivityRoute {
                job: ActivityJob::Synthesis,
                lane: ActivityLane::Anthropic,
                model: None,
                // Opaque routing label: passed through verbatim, never parsed.
                mode: "pinned_unavailable".to_string(),
                available: false,
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"pinned_unavailable\""));
        assert!(json.contains("\"on_device\""));
        let back: ActivityResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, response);
    }
}
