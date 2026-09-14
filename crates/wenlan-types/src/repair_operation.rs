// SPDX-License-Identifier: Apache-2.0
//! Durable status of one exact prepared repair. These responses never apply it.

use crate::repair::{RepairApplyReceipt, RepairDigest, RepairVerificationReceipt};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairOperationStatus {
    pub manifest_id: String,
    pub manifest_digest: RepairDigest,
    pub state: RepairOperationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepairOperationState {
    Prepared,
    InProgress,
    Indeterminate,
    AppliedUnverified {
        apply_receipt: Box<RepairApplyReceipt>,
    },
    Verified {
        apply_receipt: Box<RepairApplyReceipt>,
        verification_receipt: Box<RepairVerificationReceipt>,
    },
    Cancelled {
        cancelled_at: i64,
    },
}
