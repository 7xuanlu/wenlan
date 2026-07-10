// SPDX-License-Identifier: Apache-2.0
use super::contract::{LintContractError, LINT_MAX_EVIDENCE_PER_CHECK};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintValidationMethod {
    ExactAggregate,
    FullEnumeration,
    IntrinsicSample,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintCoverage {
    method: LintValidationMethod,
    authorized_denominator: u64,
    evaluated: u64,
    evidence_cap: u16,
    truncated: bool,
    evidence_returned: u64,
}

#[derive(Deserialize)]
struct LintCoverageWire {
    method: LintValidationMethod,
    authorized_denominator: u64,
    evaluated: u64,
    evidence_cap: u16,
    truncated: bool,
    evidence_returned: u64,
}

impl LintCoverage {
    pub fn new(
        method: LintValidationMethod,
        authorized_denominator: u64,
        evaluated: u64,
        evidence_cap: u16,
        truncated: bool,
        evidence_returned: u64,
    ) -> Result<Self, LintContractError> {
        let coverage = Self {
            method,
            authorized_denominator,
            evaluated,
            evidence_cap,
            truncated,
            evidence_returned,
        };
        coverage.validate(
            usize::try_from(evidence_returned).map_err(|_| LintContractError::InvalidCoverage)?,
        )?;
        Ok(coverage)
    }

    pub(crate) fn validate(&self, evidence_count: usize) -> Result<(), LintContractError> {
        let evidence_count =
            u64::try_from(evidence_count).map_err(|_| LintContractError::InvalidCoverage)?;
        let covers_authorized_population = match self.method {
            LintValidationMethod::FullEnumeration => self.evaluated == self.authorized_denominator,
            LintValidationMethod::ExactAggregate | LintValidationMethod::IntrinsicSample => true,
        };
        if !covers_authorized_population
            || self.evaluated > self.authorized_denominator
            || self.evidence_cap != LINT_MAX_EVIDENCE_PER_CHECK
            || self.evidence_returned != evidence_count
            || evidence_count > u64::from(self.evidence_cap)
        {
            Err(LintContractError::InvalidCoverage)
        } else {
            Ok(())
        }
    }

    pub(crate) const fn authorized_denominator(&self) -> u64 {
        self.authorized_denominator
    }

    pub const fn method(&self) -> LintValidationMethod {
        self.method
    }

    pub const fn denominator(&self) -> u64 {
        self.authorized_denominator
    }

    pub const fn evaluated(&self) -> u64 {
        self.evaluated
    }

    pub const fn evidence_cap(&self) -> u16 {
        self.evidence_cap
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    pub const fn evidence_returned(&self) -> u64 {
        self.evidence_returned
    }
}

impl<'de> Deserialize<'de> for LintCoverage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LintCoverageWire::deserialize(deserializer)?;
        Self::new(
            wire.method,
            wire.authorized_denominator,
            wire.evaluated,
            wire.evidence_cap,
            wire.truncated,
            wire.evidence_returned,
        )
        .map_err(D::Error::custom)
    }
}
