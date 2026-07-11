use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wenlan_types::lint::{
    LintContractError, LintCoverage, LintOpaqueId, LintScope, LintValidationMethod,
    LINT_MAX_EVIDENCE_PER_CHECK,
};

use super::pages::fs::PageScan;
use super::snapshot::LintReadSnapshot;

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
    epoch_seconds: i64,
}

impl LintClock {
    pub fn capture() -> Self {
        Self {
            started: Instant::now(),
            fixed: false,
            epoch_seconds: chrono::Utc::now().timestamp(),
        }
    }

    #[cfg(test)]
    pub fn fixed() -> Self {
        Self::fixed_at(0)
    }

    #[cfg(test)]
    pub fn fixed_at(epoch_seconds: i64) -> Self {
        Self {
            started: Instant::now(),
            fixed: true,
            epoch_seconds,
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

    pub(crate) fn epoch_seconds(&self) -> i64 {
        self.epoch_seconds
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScopeFilter {
    Global,
    Registered(String),
    Uncategorized,
}

impl ScopeFilter {
    pub(crate) fn is_selected(&self) -> bool {
        !matches!(self, Self::Global)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopulationBasis {
    Global,
    SelectedScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PopulationReceipt {
    pub(crate) basis: PopulationBasis,
    pub(crate) denominator: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("lint scope population accounting failed")]
pub(crate) struct PopulationLedgerError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AppliedScope {
    report: LintScope,
    filter: ScopeFilter,
}

impl AppliedScope {
    pub(crate) fn global() -> Self {
        Self {
            report: LintScope::global(),
            filter: ScopeFilter::Global,
        }
    }

    pub(crate) fn registered(opaque: LintOpaqueId, workspace: String) -> Self {
        Self {
            report: LintScope::registered(opaque),
            filter: ScopeFilter::Registered(workspace),
        }
    }

    pub(crate) fn uncategorized() -> Self {
        Self {
            report: LintScope::uncategorized(),
            filter: ScopeFilter::Uncategorized,
        }
    }

    pub(crate) fn into_report(self) -> LintScope {
        self.report
    }

    pub(crate) fn filter(&self) -> &ScopeFilter {
        &self.filter
    }
}

pub(crate) struct LintContext<'run, 'database> {
    snapshot: &'run LintReadSnapshot<'database>,
    scope: &'run AppliedScope,
    page_scan: Option<&'run PageScan>,
    clock: &'run LintClock,
    gate: &'run ExecutionGate,
    populations: Mutex<BTreeMap<&'static str, PopulationReceipt>>,
}

impl<'run, 'database> LintContext<'run, 'database> {
    pub(crate) fn new(
        snapshot: &'run LintReadSnapshot<'database>,
        scope: &'run AppliedScope,
        page_scan: Option<&'run PageScan>,
        clock: &'run LintClock,
        gate: &'run ExecutionGate,
    ) -> Self {
        Self {
            snapshot,
            scope,
            page_scan,
            clock,
            gate,
            populations: Mutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn snapshot(&self) -> &LintReadSnapshot<'database> {
        self.snapshot
    }

    pub(crate) fn scope(&self) -> &AppliedScope {
        self.scope
    }

    pub(crate) fn page_scan(&self) -> Option<&PageScan> {
        self.page_scan
    }

    pub(crate) fn clock(&self) -> &LintClock {
        self.clock
    }

    pub(crate) fn gate(&self) -> &ExecutionGate {
        self.gate
    }

    pub(crate) fn record_population(
        &self,
        check_id: &'static str,
        basis: PopulationBasis,
        denominator: u64,
    ) -> Result<(), PopulationLedgerError> {
        let mut populations = self.populations.lock().map_err(|_| PopulationLedgerError)?;
        if populations
            .insert(check_id, PopulationReceipt { basis, denominator })
            .is_some()
        {
            return Err(PopulationLedgerError);
        }
        Ok(())
    }

    pub(crate) fn population(
        &self,
        check_id: &str,
    ) -> Result<Option<PopulationReceipt>, PopulationLedgerError> {
        self.populations
            .lock()
            .map(|populations| populations.get(check_id).copied())
            .map_err(|_| PopulationLedgerError)
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
