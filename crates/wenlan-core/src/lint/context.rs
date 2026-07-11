use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use wenlan_types::lint::{
    LintContractError, LintCoverage, LintValidationMethod, LINT_MAX_EVIDENCE_PER_CHECK,
};

#[derive(Debug, Clone)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct LintClock {
    started: Instant,
    fixed: bool,
}

impl LintClock {
    pub fn capture() -> Self {
        Self {
            started: Instant::now(),
            fixed: false,
        }
    }

    #[cfg(test)]
    pub fn fixed() -> Self {
        Self {
            started: Instant::now(),
            fixed: true,
        }
    }

    pub fn elapsed(&self) -> Duration {
        if self.fixed {
            Duration::ZERO
        } else {
            self.started.elapsed()
        }
    }

    pub fn duration_ms(&self) -> u64 {
        u64::try_from(self.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExecutionGateError {
    #[error("lint execution canceled")]
    Canceled,
    #[error("lint execution budget exceeded")]
    BudgetExceeded,
}

pub struct ExecutionGate {
    cancellation: CancellationToken,
}

impl ExecutionGate {
    pub const RUN_BUDGET: Duration = Duration::from_secs(15);
    pub const PAGE_BUDGET: Duration = Duration::from_secs(5);

    pub fn new(cancellation: CancellationToken) -> Self {
        Self { cancellation }
    }

    pub fn check(&self, page_elapsed: Duration) -> Result<(), ExecutionGateError> {
        if self.cancellation.is_cancelled() {
            Err(ExecutionGateError::Canceled)
        } else if page_elapsed > Self::PAGE_BUDGET {
            Err(ExecutionGateError::BudgetExceeded)
        } else {
            Ok(())
        }
    }

    pub fn check_run(&self, elapsed: Duration) -> Result<(), ExecutionGateError> {
        if self.cancellation.is_cancelled() {
            Err(ExecutionGateError::Canceled)
        } else if elapsed > Self::RUN_BUDGET {
            Err(ExecutionGateError::BudgetExceeded)
        } else {
            Ok(())
        }
    }
}

pub struct PopulationAccounting {
    population_total: u64,
    validated: BTreeSet<u64>,
    defects: Vec<u64>,
}

impl PopulationAccounting {
    pub fn new(population_total: u64) -> Self {
        Self {
            population_total,
            validated: BTreeSet::new(),
            defects: Vec::new(),
        }
    }

    pub fn validate(&mut self, ordinal: u64, valid: bool) {
        if ordinal == 0 || ordinal > self.population_total || !self.validated.insert(ordinal) {
            return;
        }
        if !valid {
            self.defects.push(ordinal);
        }
    }

    pub fn evidence_ordinals(&self) -> &[u64] {
        let cap = usize::from(LINT_MAX_EVIDENCE_PER_CHECK);
        &self.defects[..self.defects.len().min(cap)]
    }

    pub fn coverage(&self) -> Result<LintCoverage, LintContractError> {
        let evidence_returned = u64::try_from(self.evidence_ordinals().len())
            .map_err(|_| LintContractError::InvalidCoverage)?;
        LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            self.population_total,
            u64::try_from(self.validated.len()).unwrap_or(u64::MAX),
            LINT_MAX_EVIDENCE_PER_CHECK,
            self.defects.len() > usize::from(LINT_MAX_EVIDENCE_PER_CHECK),
            evidence_returned,
        )
    }
}
