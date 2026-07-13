// SPDX-License-Identifier: Apache-2.0
use super::contract::{LintCommitReceipt, LintDigest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintCapabilityContext {
    DaemonOperatorEndpointUnauthenticatedUnverified,
}
impl LintCapabilityContext {
    pub const fn daemon_operator_endpoint() -> Self {
        Self::DaemonOperatorEndpointUnauthenticatedUnverified
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintDbSnapshotMode {
    TransactionalReadOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintPageSnapshotMode {
    BestEffort,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintDbSnapshotReceipt {
    mode: LintDbSnapshotMode,
    analysis_digest: LintDigest,
    post_run_digest: Option<LintDigest>,
}
impl LintDbSnapshotReceipt {
    pub fn new(
        mode: LintDbSnapshotMode,
        analysis_digest: LintDigest,
        post_run_digest: Option<LintDigest>,
    ) -> Self {
        Self {
            mode,
            analysis_digest,
            post_run_digest,
        }
    }
    pub const fn mode(&self) -> LintDbSnapshotMode {
        self.mode
    }
    pub fn analysis_digest(&self) -> &LintDigest {
        &self.analysis_digest
    }
    pub fn post_run_digest(&self) -> Option<&LintDigest> {
        self.post_run_digest.as_ref()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintPageSnapshotReceipt {
    mode: LintPageSnapshotMode,
    before_scan_digest: LintDigest,
    after_scan_digest: Option<LintDigest>,
}
impl LintPageSnapshotReceipt {
    pub fn new(
        mode: LintPageSnapshotMode,
        before_scan_digest: LintDigest,
        after_scan_digest: Option<LintDigest>,
    ) -> Self {
        Self {
            mode,
            before_scan_digest,
            after_scan_digest,
        }
    }
    pub const fn mode(&self) -> LintPageSnapshotMode {
        self.mode
    }
    pub fn before_scan_digest(&self) -> &LintDigest {
        &self.before_scan_digest
    }
    pub fn after_scan_digest(&self) -> Option<&LintDigest> {
        self.after_scan_digest.as_ref()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintSnapshotReceipts {
    db: LintDbSnapshotReceipt,
    pages: LintPageSnapshotReceipt,
}
impl LintSnapshotReceipts {
    pub fn new(db: LintDbSnapshotReceipt, pages: LintPageSnapshotReceipt) -> Self {
        Self { db, pages }
    }
    pub fn db(&self) -> &LintDbSnapshotReceipt {
        &self.db
    }
    pub fn pages(&self) -> &LintPageSnapshotReceipt {
        &self.pages
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintProducerReceipt {
    runtime_commit: Option<LintCommitReceipt>,
}
impl LintProducerReceipt {
    pub fn new(runtime_commit: Option<LintCommitReceipt>) -> Self {
        Self { runtime_commit }
    }
    pub fn runtime_commit(&self) -> Option<&LintCommitReceipt> {
        self.runtime_commit.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintOutcome {
    Pass,
    Finding,
    NotRunPrerequisite,
    InconsistentSnapshot,
    FailedToRun,
}
impl LintOutcome {
    pub(crate) const fn is_complete(self) -> bool {
        match self {
            Self::Pass | Self::Finding => true,
            Self::NotRunPrerequisite | Self::InconsistentSnapshot | Self::FailedToRun => false,
        }
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintGateEffect {
    #[default]
    Actionable,
    Advisory,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
    Info,
    Warning,
    Error,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintApplicability {
    Applicable,
    Inventory,
    ExpectedEmpty,
    NotApplicable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintPrecondition {
    Ready,
    ExpectedEmpty,
    ConfiguredOff,
    MissingPrerequisite,
    SnapshotUnstable,
}
