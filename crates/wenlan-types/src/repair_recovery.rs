// SPDX-License-Identifier: Apache-2.0
//! Durable, read-only repair recovery response.

use crate::repair::{RepairApplyReceipt, RepairManifest};
use serde::{Deserialize, Serialize};

/// The authoritative durable state a client may use to recover a pending
/// repair after losing its local progress record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairRecovery {
    pub manifest: RepairManifest,
    pub apply_receipt: Option<RepairApplyReceipt>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_recovery_fields_are_rejected() {
        let error = serde_json::from_str::<RepairRecovery>(
            r#"{"unexpected":true,"manifest":{},"apply_receipt":null}"#,
        )
        .expect_err("unknown recovery fields must not be accepted");
        assert!(error.to_string().contains("unknown field"));
    }
}
