use super::LintRunner;

impl LintRunner {
    pub fn with_observer(
        mut self,
        observer: std::sync::Arc<dyn crate::lint::observation::LintRunObserver>,
    ) -> Self {
        self.observer = observer;
        self
    }

    pub fn with_sources(mut self, sources: &[wenlan_types::sources::Source]) -> Self {
        self.operations_config =
            crate::lint::operations::OperationsRunConfig::from_sources(sources);
        self
    }

    pub fn with_runtime_observation(
        mut self,
        observation: crate::lint::runtime::RuntimeObservation,
    ) -> Self {
        self.runtime_config = self.runtime_config.with_observation(observation);
        self
    }

    #[cfg(test)]
    pub(in crate::lint) fn with_test_runtime_config(
        mut self,
        config: crate::lint::runtime::RuntimeRunConfig,
    ) -> Self {
        self.runtime_config = config;
        self
    }

    pub(super) fn page_elapsed(&self, page_started: std::time::Duration) -> std::time::Duration {
        #[cfg(test)]
        if self.scenario == Some(super::TestScenario::PageGroupTimeout) {
            return crate::lint::context::ExecutionGate::PAGE_BUDGET
                + std::time::Duration::from_millis(1);
        }
        self.clock.elapsed().saturating_sub(page_started)
    }
}
