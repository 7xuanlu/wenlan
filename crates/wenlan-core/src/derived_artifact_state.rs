use crate::db::MemoryDB;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

mod sweep;

pub(crate) fn summary_eligible_predicate(alias: &str) -> String {
    let minimum = crate::refinery::summary::min_bucket_members();
    format!(
        "{alias}.is_recap=0
         AND {alias}.supersede_mode<>'archive'
         AND {alias}.source_id NOT LIKE 'merged_%'
         AND {alias}.source_id NOT LIKE 'recap_%'
         AND {alias}.embedding IS NOT NULL
         AND EXISTS (
             SELECT 1 FROM entities owner
              WHERE owner.id={alias}.entity_id AND owner.community_id IS NOT NULL
                AND (
                    SELECT COUNT(*) FROM memories peer
                    JOIN entities peer_entity ON peer.entity_id=peer_entity.id
                    WHERE peer.source='memory' AND peer.chunk_index=0
                      AND peer.is_recap=0 AND peer.supersede_mode<>'archive'
                      AND peer.source_id NOT LIKE 'merged_%'
                      AND peer.source_id NOT LIKE 'recap_%'
                      AND peer.embedding IS NOT NULL
                      AND peer_entity.community_id=owner.community_id
                ) >= {minimum}
         )"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DerivedArtifact {
    Episode,
    Fact,
    Summary,
}

impl DerivedArtifact {
    const fn index(self) -> usize {
        match self {
            Self::Episode => 0,
            Self::Fact => 1,
            Self::Summary => 2,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct DerivedArtifactState {
    active: [AtomicU32; 3],
    generation: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DerivedArtifactSample {
    active: [u32; 3],
    generation: u64,
}

impl DerivedArtifactState {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn begin(self: &Arc<Self>, artifact: DerivedArtifact) -> DerivedArtifactGuard {
        self.active[artifact.index()].fetch_add(1, Ordering::AcqRel);
        DerivedArtifactGuard {
            state: Arc::clone(self),
            artifact,
        }
    }

    pub(crate) fn sample(&self) -> DerivedArtifactSample {
        DerivedArtifactSample {
            active: std::array::from_fn(|index| self.active[index].load(Ordering::Acquire)),
            generation: self.generation.load(Ordering::Acquire),
        }
    }
}

impl DerivedArtifactSample {
    pub(crate) const fn is_active(self, artifact: DerivedArtifact) -> bool {
        self.active[artifact.index()] > 0
    }
}

pub(crate) struct DerivedArtifactGuard {
    state: Arc<DerivedArtifactState>,
    artifact: DerivedArtifact,
}

impl Drop for DerivedArtifactGuard {
    fn drop(&mut self) {
        self.state.active[self.artifact.index()].fetch_sub(1, Ordering::AcqRel);
        self.state.generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl MemoryDB {
    pub(crate) fn begin_derived_artifact_write(
        &self,
        artifact: DerivedArtifact,
    ) -> DerivedArtifactGuard {
        self.derived_artifact_state.begin(artifact)
    }

    pub(crate) fn derived_artifact_sample(&self) -> DerivedArtifactSample {
        self.derived_artifact_state.sample()
    }
}
