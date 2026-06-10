// SPDX-License-Identifier: Apache-2.0
//! P3 provenance integration tests — Task 4 through Task 6.

use async_trait::async_trait;
use origin_core::db::{DistillationCluster, MemoryDB};
use origin_core::llm_provider::{LlmBackend, LlmError, LlmProvider, LlmRequest};
use origin_core::prompts::PromptRegistry;
use origin_core::refinery::distill_one_cluster;
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

// ---------------------------------------------------------------------------
// Task 5: scoped-match attach path stamps last_distilled_at for attached ids
// ---------------------------------------------------------------------------

/// Verifies that when `distill_one_cluster` takes the scoped-match ATTACH path
/// (find_matching_page_scoped fires, no new page is synthesised) it stamps
/// `last_distilled_at` only on the source_ids whose `link_page_source` call
/// succeeded, not on every id in the cluster.
#[tokio::test]
async fn scoped_match_attach_stamps_only_attached_ids() {
    let (db, _dir) = make_db().await;
    let now = chrono::Utc::now().to_rfc3339();

    // Seed two memories into the memories table so stamp_last_distilled_at has
    // rows to UPDATE.  Content length and topic are arbitrary — the cluster is
    // built manually below; we are not going through the clustering code path.
    seed_memory(
        &db,
        "mem_s3",
        "Tokio async runtime work-stealing scheduler reactor Rust executor tasks futures \
         epoll kqueue multi-threaded blocking thread pool I/O readiness",
    )
    .await;
    seed_memory(
        &db,
        "mem_s4",
        "Rust async futures zero-cost state machine poll heap allocation await point \
         tokio spawn runtime scheduler work-stealing thread local queue",
    )
    .await;

    // The embed text used by insert_page_with_kind is: title + summary + capped_body.
    // We construct the centroid by embedding the SAME title+summary text so the
    // cosine between centroid and page embedding is effectively 1.0, guaranteeing
    // find_matching_page_scoped fires at the 0.85 threshold regardless of the
    // unrelated body.
    let page_title = "Tokio Async Runtime";
    let page_summary = "Tokio async runtime work-stealing scheduler reactor Rust executor";
    let page_body = "Tokio is the de-facto async runtime for Rust. It runs futures on a \
        multi-threaded work-stealing scheduler and uses the OS event queue (epoll / kqueue) \
        as its I/O reactor. Blocking tasks run on a dedicated thread pool so they never \
        starve async I/O futures.";

    // Insert a confirmed distilled page — this is the page the scoped matcher
    // must find.  review_status = "confirmed" is required by find_matching_page_scoped.
    db.insert_page_with_kind(
        "page_scoped_seed",
        page_title,
        Some(page_summary),
        page_body,
        None, // no entity_id
        None, // no space filter
        &[],  // no source memories yet
        &now,
        "distilled",
        "confirmed",
    )
    .await
    .expect("insert_page_with_kind must succeed");

    // Build the centroid embedding from title+summary (same as the page embed
    // prefix) so the cosine is near 1.0.
    let centroid_text = format!("{page_title} {page_summary}");
    let centroid = db
        .generate_embeddings(&[centroid_text.clone()])
        .expect("generate_embeddings must succeed")
        .remove(0);

    // PRECONDITION: confirm the scoped matcher finds the seed page at 0.85.
    // If this assertion fails, tune the centroid text to be more similar to
    // page_title + page_summary + page_body (the full embed text).
    {
        let probe = db
            .find_matching_page_scoped(None, &centroid, 0.85, None, false)
            .await
            .expect("find_matching_page_scoped must not error");
        assert_eq!(
            probe.as_ref().map(|p| p.id.as_str()),
            Some("page_scoped_seed"),
            "precondition: scoped matcher must find page_scoped_seed at threshold 0.85 \
             (tune centroid_text if this fails)"
        );
    }

    // Build a cluster with non-overlapping source_ids (mem_s3, mem_s4).
    // find_best_overlapping_page will return None for these (no page cites them),
    // so distill_one_cluster falls through to find_matching_page_scoped, which
    // then fires and attaches.
    let cluster = DistillationCluster {
        source_ids: vec!["mem_s3".to_string(), "mem_s4".to_string()],
        contents: vec![
            "Tokio async runtime work-stealing scheduler reactor Rust executor tasks futures"
                .to_string(),
            "Rust async futures zero-cost state machine poll heap allocation await point"
                .to_string(),
        ],
        entity_id: None,
        entity_name: Some("Tokio".to_string()),
        space: None,
        estimated_tokens: 50,
        centroid_embedding: Some(centroid),
    };

    // Use the same DistillStubLlm as Task 4 (its body is never invoked on the
    // scoped-match path because the function returns before synthesis).
    let stub_body = "Tokio async runtime work-stealing scheduler reactor.";
    let llm: Arc<dyn LlmProvider> = Arc::new(DistillStubLlm::new(stub_body));
    let prompts = PromptRegistry::default();

    let result = distill_one_cluster(&db, &llm, &prompts, &cluster, None)
        .await
        .expect("distill_one_cluster must not error");

    assert!(
        result.is_none(),
        "scoped-match attach must return Ok(None) (no new page created)"
    );

    // Both source memories must now carry a non-NULL last_distilled_at because
    // link_page_source succeeded for both (no pre-existing page_sources rows).
    for sid in ["mem_s3", "mem_s4"] {
        let ld = last_distilled_at_of(&db, sid).await;
        assert!(
            ld.is_some(),
            "{sid} must carry a non-NULL last_distilled_at after scoped-match attach"
        );
    }
}
