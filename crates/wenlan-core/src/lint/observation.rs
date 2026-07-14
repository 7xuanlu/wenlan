#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintRunEvent {
    ScopeValidation,
    TransactionQuery,
    PageScan,
    AggregateChecks,
    ReportBuild,
}

pub trait LintRunObserver: Send + Sync {
    fn observe(&self, event: LintRunEvent);
}

#[derive(Default)]
pub struct NoopLintRunObserver;

impl LintRunObserver for NoopLintRunObserver {
    fn observe(&self, _event: LintRunEvent) {}
}
