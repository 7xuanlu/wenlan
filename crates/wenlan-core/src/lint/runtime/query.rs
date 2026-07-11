use super::result::{Assessment, PendingAssessment};
use super::{RuntimeReadiness, RuntimeRunConfig, StatusFilesObservation, WorkingMemoryObservation};
use super::{INDEXES, PROVIDERS, SCHEMA, STATUS, WORKER};
use crate::lint::context::LintContext;

const WORKING_MEMORY_MAX_AGE_SECONDS: u64 = 900;

pub(super) struct RuntimeSnapshot {
    schema: Result<super::schema::SchemaSnapshot, ()>,
    files_indexed: Result<u64, ()>,
}

impl RuntimeSnapshot {
    pub(super) fn assessments(&self, config: &RuntimeRunConfig) -> [PendingAssessment; 5] {
        let (schema, indexes) = match &self.schema {
            Ok(snapshot) => (
                PendingAssessment::Ready(Assessment::new(
                    SCHEMA,
                    snapshot.schema_population(),
                    snapshot.schema_affected(),
                )),
                PendingAssessment::Ready(Assessment::new(
                    INDEXES,
                    snapshot.search_population(),
                    snapshot.search_affected(),
                )),
            ),
            Err(()) => (
                PendingAssessment::Failed(SCHEMA),
                PendingAssessment::Failed(INDEXES),
            ),
        };
        let provider_affected = config
            .snapshot
            .providers
            .iter()
            .filter(|request| {
                !config.observation.providers.iter().any(|observed| {
                    observed.class == request.class
                        && observed.model_id == request.model_id
                        && observed.readiness == RuntimeReadiness::Ready
                })
            })
            .count();
        let reranker_affected = config
            .snapshot
            .rerankers
            .iter()
            .filter(|request| {
                !config.observation.rerankers.iter().any(|observed| {
                    observed.path == request.path
                        && observed.model_id == request.model_id
                        && observed.readiness == RuntimeReadiness::Ready
                })
            })
            .count();
        let requested = config.snapshot.providers.len() + config.snapshot.rerankers.len();
        let observed = config
            .observation
            .providers
            .iter()
            .filter(|provider| provider.readiness == RuntimeReadiness::Ready)
            .count()
            + config
                .observation
                .rerankers
                .iter()
                .filter(|reranker| reranker.readiness == RuntimeReadiness::Ready)
                .count();
        let providers = PendingAssessment::Ready(
            Assessment::new(
                PROVIDERS,
                u64::try_from(requested).unwrap_or(u64::MAX),
                u64::try_from(provider_affected + reranker_affected).unwrap_or(u64::MAX),
            )
            .with_observed(u64::try_from(observed).unwrap_or(u64::MAX)),
        );
        let status = match (self.files_indexed, config.observation.status_files) {
            (Ok(files_indexed), StatusFilesObservation::Direct(status_files)) => {
                PendingAssessment::Ready(
                    Assessment::new(STATUS, 1, u64::from(status_files != files_indexed))
                        .with_observed(files_indexed),
                )
            }
            (Ok(files_indexed), StatusFilesObservation::Unavailable) => {
                PendingAssessment::Ready(Assessment::new(STATUS, 0, 0).with_observed(files_indexed))
            }
            (Err(()), _) | (_, StatusFilesObservation::DirectError { .. }) => {
                PendingAssessment::Failed(STATUS)
            }
        };
        let working = working_memory(
            config.observation.working_memory,
            config.clock_epoch_seconds,
        );
        let worker = PendingAssessment::Ready(
            Assessment::new(
                WORKER,
                u64::from(config.observation.ingest_worker_closed.is_some()) + working.population,
                u64::from(config.observation.ingest_worker_closed == Some(true)) + working.affected,
            )
            .with_metric(
                wenlan_types::lint::LintMetricCode::WorkingMemoryTelemetryRows,
                working.rows,
            )
            .with_metric(
                wenlan_types::lint::LintMetricCode::WorkingMemoryTelemetryUnavailable,
                working.unavailable,
            )
            .with_metric(
                wenlan_types::lint::LintMetricCode::WorkingMemoryNewestAgeSeconds,
                working.age,
            ),
        );
        [schema, indexes, providers, status, worker]
    }
}

pub(super) async fn load(
    context: &LintContext<'_, '_>,
    _config: &RuntimeRunConfig,
) -> RuntimeSnapshot {
    #[cfg(test)]
    let forced_failure = _config.force_query_failure;
    #[cfg(not(test))]
    let forced_failure = false;
    let schema = if forced_failure {
        Err(())
    } else {
        super::schema::load(context).await
    };
    let files_indexed = if forced_failure {
        Err(())
    } else {
        scalar(
            context,
            "SELECT COUNT(*) FROM memories WHERE source != 'episode'",
        )
        .await
    };
    RuntimeSnapshot {
        schema,
        files_indexed,
    }
}

struct WorkingMemoryAssessment {
    population: u64,
    affected: u64,
    rows: u64,
    unavailable: u64,
    age: u64,
}

fn working_memory(observation: WorkingMemoryObservation, now: i64) -> WorkingMemoryAssessment {
    match observation {
        WorkingMemoryObservation::Unavailable => WorkingMemoryAssessment {
            population: 0,
            affected: 0,
            rows: 0,
            unavailable: 1,
            age: 0,
        },
        WorkingMemoryObservation::Available {
            entries,
            newest_timestamp,
        } => {
            let age = newest_timestamp
                .and_then(|timestamp| now.checked_sub(timestamp))
                .and_then(|value| u64::try_from(value).ok())
                .unwrap_or(0);
            let invalid = (entries == 0 && newest_timestamp.is_some())
                || (entries > 0
                    && newest_timestamp.is_none_or(|timestamp| {
                        timestamp > now || age > WORKING_MEMORY_MAX_AGE_SECONDS
                    }));
            WorkingMemoryAssessment {
                population: 1,
                affected: u64::from(invalid),
                rows: entries,
                unavailable: 0,
                age,
            }
        }
    }
}

async fn scalar(context: &LintContext<'_, '_>, sql: &str) -> Result<u64, ()> {
    let mut rows = context
        .snapshot()
        .query(sql, libsql::params::Params::None)
        .await
        .map_err(|_| ())?;
    let value = rows
        .next()
        .await
        .map_err(|_| ())?
        .ok_or(())?
        .get::<i64>(0)
        .map_err(|_| ())?;
    u64::try_from(value).map_err(|_| ())
}
