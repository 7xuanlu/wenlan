use crate::lint::context::{LintClock, LintContext};
use crate::lint::snapshot::LintReadSnapshot;

pub(super) struct OperationsReadContext<'run, 'database> {
    snapshot: &'run LintReadSnapshot<'database>,
    clock: &'run LintClock,
}

impl<'run, 'database> OperationsReadContext<'run, 'database> {
    pub(super) fn from_lint(context: &'run LintContext<'_, 'database>) -> Self {
        Self {
            snapshot: context.snapshot(),
            clock: context.clock(),
        }
    }

    pub(super) const fn snapshot(&self) -> &LintReadSnapshot<'database> {
        self.snapshot
    }

    pub(super) const fn clock(&self) -> &LintClock {
        self.clock
    }
}
