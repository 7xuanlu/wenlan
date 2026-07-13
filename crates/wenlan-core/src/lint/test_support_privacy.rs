use serde_json::{Map, Number, Value};
use std::collections::BTreeSet;

const FIXED_OBSERVED_AT: i64 = 1_700_000_000;
const CANARIES: [&str; 11] = [
    "CANARY_MEMORY_CONTENT_7f31",
    "CANARY_PAGE_TITLE_7f31",
    "CANARY_SESSION_SUMMARY_7f31",
    "canary-private-filename-7f31.md",
    "MALFORMED_STATE_ID_7f31",
    "MALFORMED_STATE_VALUE_7f31",
    "/private/source/CANARY_IMPORT_7f31.md",
    "/Users/canary/CANARY_HOME_7f31",
    "private-host-7f31.invalid",
    "CANARY_ENV_VALUE_7f31",
    "nested error at /private/CANARY_ERROR_7f31",
];

pub(crate) struct PrivacyCanaries;

impl PrivacyCanaries {
    pub(crate) const fn all() -> &'static [&'static str] {
        &CANARIES
    }
}

pub(crate) fn assert_no_privacy_canaries(output: &str) {
    for canary in PrivacyCanaries::all() {
        assert!(!output.contains(canary), "forbidden privacy canary found");
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LintClock {
    observed_at: i64,
}

impl LintClock {
    pub(crate) const fn fixed() -> Self {
        Self {
            observed_at: FIXED_OBSERVED_AT,
        }
    }

    pub(crate) const fn observed_at(self) -> i64 {
        self.observed_at
    }
}

pub(crate) fn normalize_json(value: &Value, clock: LintClock) -> Value {
    normalize_value(value, clock, None)
}

fn normalize_value(value: &Value, clock: LintClock, key: Option<&str>) -> Value {
    if key == Some("observed_at") {
        return Value::Number(Number::from(clock.observed_at()));
    }
    if key.is_some_and(|name| name == "duration_ms" || name.ends_with("_duration_ms")) {
        return Value::Number(Number::from(0));
    }
    match value {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|value| normalize_value(value, clock, None))
                .collect(),
        ),
        Value::Object(values) => {
            let mut normalized = Map::new();
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                normalized.insert(key.clone(), normalize_value(&values[key], clock, Some(key)));
            }
            Value::Object(normalized)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => value.clone(),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PopulationRow {
    ordinal: u64,
    valid: bool,
}

pub(crate) fn population_after_sample_cap() -> Vec<PopulationRow> {
    let last_ordinal = u64::from(wenlan_types::lint::LINT_MAX_EVIDENCE_PER_CHECK) + 1;
    (1..=last_ordinal)
        .map(|ordinal| PopulationRow {
            ordinal,
            valid: false,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PopulationValidation {
    population_total: u64,
    validated: BTreeSet<u64>,
    examples: Vec<u64>,
    failed: bool,
    truncated: bool,
}

impl PopulationValidation {
    pub(crate) const fn failed(&self) -> bool {
        self.failed
    }

    pub(crate) const fn population_total(&self) -> u64 {
        self.population_total
    }

    pub(crate) fn validated_total(&self) -> u64 {
        u64::try_from(self.validated.len()).unwrap_or(u64::MAX)
    }

    pub(crate) fn validated_row(&self, ordinal: u64) -> bool {
        self.validated.contains(&ordinal)
    }

    pub(crate) fn examples(&self) -> &[u64] {
        &self.examples
    }

    pub(crate) const fn truncated(&self) -> bool {
        self.truncated
    }
}

pub(crate) fn validate_population(rows: &[PopulationRow]) -> PopulationValidation {
    let evidence_cap = usize::from(wenlan_types::lint::LINT_MAX_EVIDENCE_PER_CHECK);
    let population_total = u64::try_from(rows.len()).unwrap_or(u64::MAX);
    let validated = rows.iter().map(|row| row.ordinal).collect::<BTreeSet<_>>();
    let defects = rows
        .iter()
        .filter(|row| !row.valid)
        .map(|row| row.ordinal)
        .collect::<Vec<_>>();
    PopulationValidation {
        population_total,
        validated,
        examples: defects.iter().take(evidence_cap).copied().collect(),
        failed: !defects.is_empty(),
        truncated: defects.len() > evidence_cap,
    }
}
