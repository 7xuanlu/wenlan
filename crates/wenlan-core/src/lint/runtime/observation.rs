#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderClass {
    AnthropicRoutine,
    AnthropicSynthesis,
    External,
    OnDevice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RerankerPath {
    Light,
    Deep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeReadiness {
    Ready,
    Failed,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilesObservation {
    Unavailable,
    Direct(u64),
    DirectError { fallback_files_indexed: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingMemoryObservation {
    Unavailable,
    Available {
        entries: u64,
        newest_timestamp: Option<i64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProviderObservation {
    pub(super) class: ProviderClass,
    pub(super) model_id: String,
    pub(super) readiness: RuntimeReadiness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RerankerObservation {
    pub(super) path: RerankerPath,
    pub(super) model_id: String,
    pub(super) readiness: RuntimeReadiness,
}

#[derive(Debug, Clone)]
pub struct RuntimeObservation {
    pub(super) providers: Vec<ProviderObservation>,
    pub(super) rerankers: Vec<RerankerObservation>,
    pub(super) ingest_worker_closed: Option<bool>,
    pub(super) status_files: StatusFilesObservation,
    pub(super) working_memory: WorkingMemoryObservation,
}

impl RuntimeObservation {
    pub const fn unavailable() -> Self {
        Self {
            providers: Vec::new(),
            rerankers: Vec::new(),
            ingest_worker_closed: None,
            status_files: StatusFilesObservation::Unavailable,
            working_memory: WorkingMemoryObservation::Unavailable,
        }
    }

    pub fn with_provider(
        mut self,
        class: ProviderClass,
        model_id: impl Into<String>,
        readiness: RuntimeReadiness,
    ) -> Self {
        self.providers.push(ProviderObservation {
            class,
            model_id: model_id.into(),
            readiness,
        });
        self
    }

    pub fn with_reranker(
        mut self,
        path: RerankerPath,
        model_id: impl Into<String>,
        readiness: RuntimeReadiness,
    ) -> Self {
        self.rerankers.push(RerankerObservation {
            path,
            model_id: model_id.into(),
            readiness,
        });
        self
    }

    pub const fn with_ingest_worker_closed(mut self, value: bool) -> Self {
        self.ingest_worker_closed = Some(value);
        self
    }

    pub const fn with_status_files(mut self, value: StatusFilesObservation) -> Self {
        self.status_files = value;
        self
    }

    pub const fn with_working_memory(mut self, value: WorkingMemoryObservation) -> Self {
        self.working_memory = value;
        self
    }

    pub const fn ingest_worker_closed(&self) -> Option<bool> {
        self.ingest_worker_closed
    }

    pub const fn status_files(&self) -> StatusFilesObservation {
        self.status_files
    }

    pub fn provider_readiness(
        &self,
        class: ProviderClass,
        model_id: &str,
    ) -> Option<RuntimeReadiness> {
        self.providers
            .iter()
            .find(|provider| provider.class == class && provider.model_id == model_id)
            .map(|provider| provider.readiness)
    }

    pub fn reranker_readiness(
        &self,
        path: RerankerPath,
        model_id: &str,
    ) -> Option<RuntimeReadiness> {
        self.rerankers
            .iter()
            .find(|reranker| reranker.path == path && reranker.model_id == model_id)
            .map(|reranker| reranker.readiness)
    }

    #[cfg(test)]
    pub(crate) fn open(status_files_indexed: u64) -> Self {
        Self::unavailable()
            .with_ingest_worker_closed(false)
            .with_status_files(StatusFilesObservation::Direct(status_files_indexed))
    }

    #[cfg(test)]
    pub(crate) fn closed(status_files_indexed: u64) -> Self {
        Self::open(status_files_indexed).with_ingest_worker_closed(true)
    }
}

impl Default for RuntimeObservation {
    fn default() -> Self {
        Self::unavailable()
    }
}
