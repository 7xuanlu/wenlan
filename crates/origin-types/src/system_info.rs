// SPDX-License-Identifier: Apache-2.0
//! System info wire type. Reported by daemon + sometimes detected client-side.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub total_ram_gb: f64,
    pub available_ram_gb: f64,
    pub has_metal: bool,
    pub has_cuda: bool,
    pub os: String,
    pub arch: String,
    pub recommended_builtin: String,
}
