// SPDX-License-Identifier: Apache-2.0
//! Eval layer classification — which surface a baseline measures.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Which layer of the production query path an eval baseline covers.
///
/// L1Db: in-process `MemoryDB::search_memory()` direct.
/// L2Http: HTTP through spawned `wenlan-server` daemon.
/// L3Mcp: stdio MCP roundtrip through spawned `wenlan-mcp`.
/// L4Skill: reserved for skill+model in-loop (not built yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalLayer {
    L1Db,
    L2Http,
    L3Mcp,
    L4Skill,
}

impl EvalLayer {
    pub fn as_path_component(&self) -> &'static str {
        match self {
            EvalLayer::L1Db => "l1_db",
            EvalLayer::L2Http => "l2_http",
            EvalLayer::L3Mcp => "l3_mcp",
            EvalLayer::L4Skill => "l4_skill",
        }
    }
}

impl fmt::Display for EvalLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_path_component())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_components_distinct() {
        let layers = [
            EvalLayer::L1Db,
            EvalLayer::L2Http,
            EvalLayer::L3Mcp,
            EvalLayer::L4Skill,
        ];
        let mut seen = std::collections::HashSet::new();
        for l in layers {
            assert!(
                seen.insert(l.as_path_component()),
                "duplicate path for {:?}",
                l
            );
        }
    }

    #[test]
    fn serde_roundtrip() {
        let l = EvalLayer::L2Http;
        let json = serde_json::to_string(&l).unwrap();
        assert_eq!(json, r#""l2_http""#);
        let parsed: EvalLayer = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, EvalLayer::L2Http);
    }
}
