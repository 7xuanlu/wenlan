// SPDX-License-Identifier: Apache-2.0
//! Client identity for preparation, before a manifest ID is known.
use crate::{
    repair::RepairManifest, repair_current::PrepareCurrentRepairRequest,
    repair_operation::RepairOperationStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairPrepareOperationRequest {
    pub operation_id: String,
    pub request: PrepareCurrentRepairRequest,
}

impl RepairPrepareOperationRequest {
    pub fn validate(&self) -> Result<(), String> {
        let id = self.operation_id.as_bytes();
        if id.len() != 36
            || !id.iter().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    *b == b'-'
                } else {
                    b.is_ascii_digit() || (b'a'..=b'f').contains(b)
                }
            })
        {
            return Err("invalid_repair_prepare_operation_id".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairPrepareOperationStatus {
    pub operation_id: String,
    pub state: RepairPrepareOperationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepairPrepareOperationState {
    NotStarted,
    InProgress,
    Interrupted,
    Ready {
        manifest: Box<RepairManifest>,
        operation: Box<RepairOperationStatus>,
    },
    Cancelled {
        cancelled_at: i64,
    },
}
