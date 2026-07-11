use axum::body::Body;
use axum::http::{Method, Request};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use wenlan_core::db::MemoryDB;
use wenlan_core::events::{EventEmitter, NoopEmitter};
use wenlan_core::lint::observation::{LintRunEvent, LintRunObserver};
use wenlan_types::sources::Source;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Fingerprint {
    database: [u8; 32],
    pages: [u8; 32],
    projection_generation: u64,
}

pub(super) struct Fixture {
    pub(super) app: crate::router::AppRouter,
    pub(super) db: Arc<MemoryDB>,
    pub(super) lint_events: LintEventSpy,
    pub(super) root: tempfile::TempDir,
}

#[derive(Clone, Default)]
pub(super) struct LintEventSpy(Arc<Mutex<Vec<LintRunEvent>>>);

impl LintEventSpy {
    pub(super) fn events(&self) -> Vec<LintRunEvent> {
        self.0.lock().expect("lint event spy").clone()
    }
}

impl LintRunObserver for LintEventSpy {
    fn observe(&self, event: LintRunEvent) {
        self.0.lock().expect("lint event spy").push(event);
    }
}

impl Fixture {
    pub(super) async fn new(sources: Vec<Source>, page_root: Option<PathBuf>) -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let emitter: Arc<dyn EventEmitter> = Arc::new(NoopEmitter);
        let db = Arc::new(MemoryDB::new(root.path(), emitter).await.expect("database"));
        let lint_events = LintEventSpy::default();
        let state = crate::state::ServerState {
            db: Some(Arc::clone(&db)),
            lint_config: crate::state::LintServerConfig::new(sources, page_root),
            lint_observer: Arc::new(lint_events.clone()),
            ..Default::default()
        };
        let app = crate::router::build_router(Arc::new(RwLock::new(state)));
        Self {
            app,
            db,
            lint_events,
            root,
        }
    }

    pub(super) async fn fingerprint(&self) -> Fingerprint {
        Fingerprint {
            database: database_fingerprint(&self.db).await,
            pages: tree_fingerprint(self.root.path()),
            projection_generation: self.db.page_projection_tracker().sample().generation(),
        }
    }
}

pub(super) fn request(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request")
}

async fn database_fingerprint(db: &MemoryDB) -> [u8; 32] {
    let snapshot = db.open_lint_snapshot().await.expect("snapshot");
    let receipt = snapshot.finish().await.expect("snapshot finish");
    receipt.analysis_digest().as_bytes()
}

fn tree_fingerprint(root: &Path) -> [u8; 32] {
    let mut paths = std::fs::read_dir(root)
        .expect("root entries")
        .map(|entry| entry.expect("entry").path())
        .filter(|path| {
            !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("origin_memory.db")
                    | Some("origin_memory.db-wal")
                    | Some("origin_memory.db-shm")
            )
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut digest = Sha256::new();
    for path in paths {
        digest.update(path.as_os_str().as_encoded_bytes());
        if path.is_file() {
            digest.update(std::fs::read(path).expect("file bytes"));
        }
    }
    digest.finalize().into()
}
