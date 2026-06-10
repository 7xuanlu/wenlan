// SPDX-License-Identifier: Apache-2.0
//! P3 provenance integration tests — Task 4 through Task 6.

use async_trait::async_trait;
use origin_core::db::MemoryDB;
use origin_core::llm_provider::{LlmBackend, LlmError, LlmProvider, LlmRequest};
use origin_core::sources::RawDocument;
use origin_core::{EventEmitter, NoopEmitter};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Shared helpers (mirroring distillation_quality.rs / provenance_p2.rs)
// ---------------------------------------------------------------------------

async fn make_db() -> (Arc<MemoryDB>, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let emitter: Arc<dyn EventEmitter> = Arc::new(NoopEmitter);
    let db = MemoryDB::new(&db_path, emitter)
        .await
        .expect("MemoryDB::new");
    (Arc::new(db), dir)
}

/// Seed a raw memory via upsert.  `confirmed = None` ⇒ NULL in DB, which
/// passes the clustering filter `(confirmed = 0 OR confirmed IS NULL)`.
async fn seed_memory(db: &MemoryDB, source_id: &str, content: &str) {
    let doc = RawDocument {
        source: "memory".to_string(),
        source_id: source_id.to_string(),
        title: source_id.to_string(),
        summary: None,
        content: content.to_string(),
        url: None,
        last_modified: chrono::Utc::now().timestamp(),
        memory_type: Some("fact".to_string()),
        // Put all memories in the same space so they cluster under the same
        // topic label and the thin-cluster guard (200 chars) is computed
        // over all three together.
        space: Some("technology".to_string()),
        source_agent: Some("test-agent".to_string()),
        confidence: None,
        confirmed: None, // unconfirmed → eligible for clustering
        supersedes: None,
        pending_revision: false,
        ..Default::default()
    };
    db.upsert_documents(vec![doc]).await.expect("seed memory");
}

/// Read `last_distilled_at` for a memory directly from SQL.
/// Returns `None` if the memory doesn't exist or the column is NULL.
async fn last_distilled_at_of(db: &Arc<MemoryDB>, source_id: &str) -> Option<i64> {
    db.read_last_distilled_at(source_id)
        .await
        .expect("read_last_distilled_at")
}

// ---------------------------------------------------------------------------
// Stub LLM — always available; returns content rich enough to pass the
// 0.6 cosine-similarity hallucination gate when seeded with Rust async content.
// ---------------------------------------------------------------------------

/// A deterministic LLM stub that returns either a rich distillation body or
/// a short title, based on which prompt system hint is present.
struct DistillStubLlm {
    /// Fixed body text echoed back for distillation calls.
    body: String,
}

impl DistillStubLlm {
    fn new(body: impl Into<String>) -> Self {
        Self { body: body.into() }
    }
}

#[async_trait]
impl LlmProvider for DistillStubLlm {
    async fn generate(&self, request: LlmRequest) -> Result<String, LlmError> {
        let sys = request.system_prompt.as_deref().unwrap_or("");
        // "3-5 word" matches generate_short_title's system prompt
        // (refinery/mod.rs) — the title call every successful distill makes.
        if sys.contains("3-5 word") {
            Ok("Rust Async Runtime Internals".to_string())
        } else {
            // Distillation body — echo our pre-built content.
            Ok(self.body.clone())
        }
    }

    fn is_available(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "distill-stub"
    }

    fn backend(&self) -> LlmBackend {
        LlmBackend::OnDevice
    }
}

// ---------------------------------------------------------------------------
// Task 4: normal distill path stamps last_distilled_at
// ---------------------------------------------------------------------------

/// Seeds three thematically-similar Rust-async memories that form a single
/// cluster at the test's 0.6 similarity threshold (their pairwise cosines
/// hover near the 0.73 production default — too knife-edge to rely on).
///
/// Total content is ~640 chars which clears the 200-char thin-cluster guard.
async fn seed_rust_async_cluster(db: &MemoryDB) {
    seed_memory(
        db,
        "mem_p3_a",
        "Rust async runtime tokio executor drives futures to completion \
         using a multi-threaded work-stealing scheduler. Each thread runs \
         a local run-queue and steals tasks from siblings when idle. \
         The reactor polls I/O readiness via epoll on Linux and kqueue on macOS.",
    )
    .await;

    seed_memory(
        db,
        "mem_p3_b",
        "Tokio async tasks are spawned via tokio::spawn and scheduled onto \
         the multi-thread runtime. The runtime splits CPU-bound and I/O-bound \
         work: blocking tasks run in a dedicated thread pool to avoid starving \
         async I/O futures on the reactor threads.",
    )
    .await;

    seed_memory(
        db,
        "mem_p3_c",
        "Async Rust futures are zero-cost abstractions: a future that has never \
         been polled allocates nothing on the heap in the common case. The state \
         machine generated by the compiler stores only the locals alive across \
         await points, keeping stack usage bounded in deep async call chains.",
    )
    .await;
}

#[tokio::test]
async fn normal_distill_stamps_last_distilled_at() {
    let (db, _dir) = make_db().await;

    // Seed 3 thematically-similar memories.  They are unconfirmed (confirmed=NULL)
    // and share the same space ("technology"), so they land in the unlinked pool
    // and cluster by cosine similarity.
    seed_rust_async_cluster(&db).await;

    // Build a distillation body whose embedding will be similar to the source
    // memories (> 0.6 cosine similarity).  The content closely paraphrases the
    // seeded memories, so the hallucination gate passes.
    let body = "- TLDR: Rust async internals\n\n\
        Tokio's async runtime uses a work-stealing multi-threaded scheduler where \
        each thread maintains a local task queue and steals from sibling threads \
        when idle.  The I/O reactor polls epoll on Linux and kqueue on macOS for \
        readiness events.  Blocking tasks run on a dedicated thread pool so they \
        never starve the async futures on the reactor threads.  Async Rust futures \
        are zero-cost state machines compiled from async fn bodies; they only \
        allocate on the heap when polled, keeping memory usage proportional to \
        the depth of concurrent work.";

    let llm: Arc<dyn LlmProvider> = Arc::new(DistillStubLlm::new(body));

    let prompts = origin_core::prompts::PromptRegistry::default();
    // similarity_threshold 0.6 instead of the 0.73 default: the three seeds'
    // pairwise cosines sit near 0.73, and with exactly min_cluster_size
    // memories one flipped edge from run-to-run ONNX embedding jitter kills
    // the cluster (observed flaking 4/23 runs at the default). 0.6 gives
    // margin while still exercising the real clustering code.
    let tuning = origin_core::tuning::DistillationConfig {
        similarity_threshold: 0.6,
        ..Default::default()
    };

    let result = origin_core::refinery::distill_pages(
        &db,
        Some(&llm),
        &prompts,
        &tuning,
        None, // no knowledge-writer path
    )
    .await
    .expect("distill_pages must succeed");

    assert!(
        result > 0,
        "distill_pages must have created at least one page from the cluster (got 0); \
         check that all 3 memories are unconfirmed and share the 'technology' space"
    );

    // Every source memory must now carry a non-NULL last_distilled_at.
    for sid in ["mem_p3_a", "mem_p3_b", "mem_p3_c"] {
        let ld = last_distilled_at_of(&db, sid).await;
        assert!(
            ld.is_some(),
            "{sid} must carry a non-NULL last_distilled_at after normal distill"
        );
    }
}
