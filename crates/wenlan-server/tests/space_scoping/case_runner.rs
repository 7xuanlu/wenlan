// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use wenlan_server::sensitive_read_routes::{
    route, Method, ScopeBinding, SelectionGate, SelectorPrecedence, UnknownScopePolicy,
};

#[derive(Debug, Clone, Copy)]
pub struct ExpectedContract {
    pub method: Method,
    pub path: &'static str,
    pub precedence: SelectorPrecedence,
    pub binding: ScopeBinding,
    pub gate: SelectionGate,
}

pub const WAVE_1: &[ExpectedContract] = &[
    body(Method::Post, "/api/search"),
    body(Method::Post, "/api/context"),
    header(Method::Get, "/api/memory/recent"),
    header(Method::Get, "/api/memory/unconfirmed"),
    body(Method::Post, "/api/memory/search"),
    body(Method::Post, "/api/memory/list"),
    query(Method::Get, "/api/memory/nurture"),
    header(Method::Get, "/api/memory/pinned"),
];

const fn body(method: Method, path: &'static str) -> ExpectedContract {
    expected(method, path, SelectorPrecedence::BodyThenHeader)
}

const fn query(method: Method, path: &'static str) -> ExpectedContract {
    expected(method, path, SelectorPrecedence::QueryThenHeader)
}

const fn header(method: Method, path: &'static str) -> ExpectedContract {
    expected(method, path, SelectorPrecedence::HeaderOnly)
}

const fn expected(
    method: Method,
    path: &'static str,
    precedence: SelectorPrecedence,
) -> ExpectedContract {
    ExpectedContract {
        method,
        path,
        precedence,
        binding: ScopeBinding::MemorySpace,
        gate: SelectionGate::NotApplicable,
    }
}

pub fn assert_wave_1_catalog_contract() {
    let keys = WAVE_1
        .iter()
        .map(|case| (case.method, case.path))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        keys.len(),
        8,
        "Wave 1 registry must contain eight unique keys"
    );

    for expected in WAVE_1 {
        let actual = route(expected.method, expected.path).expect("cataloged Wave 1 route");
        assert_eq!(
            actual.selector_precedence, expected.precedence,
            "{}",
            expected.path
        );
        assert_eq!(actual.scope_binding, expected.binding, "{}", expected.path);
        assert_eq!(actual.selection_gate, expected.gate, "{}", expected.path);
        assert_eq!(
            actual.unknown_scope,
            UnknownScopePolicy::Rejected,
            "{}",
            expected.path
        );
        assert!(!actual.scope_contract_violation(), "{}", expected.path);
    }
}

pub fn assert_wave_1_executed_keys(executed: impl IntoIterator<Item = (Method, &'static str)>) {
    let expected = WAVE_1
        .iter()
        .map(|case| (case.method, case.path))
        .collect::<BTreeSet<_>>();
    let executed = executed.into_iter().collect::<Vec<_>>();
    let unique = executed.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        unique.len(),
        executed.len(),
        "duplicate executed Wave 1 key"
    );
    assert_eq!(unique, expected, "executed Wave 1 key set drifted");
}
