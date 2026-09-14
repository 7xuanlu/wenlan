// SPDX-License-Identifier: Apache-2.0
//! Instance-bound transition from cold repair recovery to normal service.

use crate::repair::{ApplyRepairRequest, RepairDigest};
use serde::{Deserialize, Serialize};

/// Process identity is measured again by the native owner before requesting a
/// transition. An instance id or digest alone does not authorize killing a PID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairRuntimeStatus {
    pub instance_id: String,
    pub pid: u32,
    pub repair_only: bool,
    pub shutdown_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeRepairRuntimeRequest {
    pub instance_id: String,
    pub apply: ApplyRepairRequest,
    pub verification_receipt_digest: RepairDigest,
}

/// The UI supplies repair authority; native code measures process identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairRuntimeResumeApproval {
    pub apply: ApplyRepairRequest,
    pub verification_receipt_digest: RepairDigest,
}

impl RepairRuntimeResumeApproval {
    pub fn for_instance(&self, instance_id: String) -> ResumeRepairRuntimeRequest {
        ResumeRepairRuntimeRequest {
            instance_id,
            apply: self.apply.clone(),
            verification_receipt_digest: self.verification_receipt_digest.clone(),
        }
    }
}
