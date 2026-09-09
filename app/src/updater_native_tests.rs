// SPDX-License-Identifier: AGPL-3.0-only
//! Real updater HTTP, signature and macOS extraction against disposable fixtures.
//! Tauri's MockRuntime supplies an AppHandle without starting the Wenlan daemon/UI.
use super::*;
use std::sync::atomic::AtomicUsize;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::{JoinHandle, JoinSet};

const ARCHIVE: &[u8] = include_bytes!("../tests/fixtures/updater/fixture.app.tar.gz");
const SIGNATURE: &str = include_str!("../tests/fixtures/updater/fixture.app.tar.gz.sig");
const PUBLIC_KEY: &str = include_str!("../tests/fixtures/updater/fixture.pub");
const OLD: &[u8] = b"Wenlan updater fixture v1\n";
const NEW: &[u8] = b"Wenlan updater fixture v2\n";

struct Server {
    endpoint: String,
    published: Arc<AtomicBool>,
    stall_manifest: Arc<AtomicBool>,
    stall_download: Arc<AtomicBool>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn start(tampered: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let endpoint = format!("{origin}/latest.json");
        let published = Arc::new(AtomicBool::new(false));
        let stall_manifest = Arc::new(AtomicBool::new(false));
        let stall_download = Arc::new(AtomicBool::new(false));
        let flags = (
            published.clone(),
            stall_manifest.clone(),
            stall_download.clone(),
        );
        let task = tokio::spawn(async move {
            let mut clients = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut socket, _) = accepted.unwrap();
                        let (published, stall_manifest, stall_download) = flags.clone();
                        let origin = origin.clone();
                        clients.spawn(async move {
                            let mut request = Vec::new();
                            let mut byte = [0u8; 1];
                            while !request.ends_with(b"\r\n\r\n") {
                                if socket.read_exact(&mut byte).await.is_err() { return; }
                                request.push(byte[0]);
                                assert!(request.len() < 16_384);
                            }
                            let download = request.starts_with(b"GET /fixture.tar.gz ");
                            if !download && stall_manifest.swap(false, Ordering::SeqCst) {
                                std::future::pending::<()>().await;
                            }
                            let (status, mut body) = if download {
                                ("200 OK", ARCHIVE.to_vec())
                            } else if published.load(Ordering::SeqCst) {
                                ("200 OK", serde_json::to_vec(&serde_json::json!({
                                    "version": "99.0.0", "url": format!("{origin}/fixture.tar.gz"),
                                    "signature": SIGNATURE.trim(), "notes": "Disposable updater fixture"
                                })).unwrap())
                            } else { ("204 No Content", Vec::new()) };
                            if download && tampered { body.push(0); }
                            let headers = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                            if socket.write_all(headers.as_bytes()).await.is_err() { return; }
                            if download && stall_download.load(Ordering::SeqCst) {
                                std::future::pending::<()>().await;
                            }
                            let _ = socket.write_all(&body).await;
                        });
                    }
                    _ = clients.join_next(), if !clients.is_empty() => {}
                }
            }
        });
        Self {
            endpoint,
            published,
            stall_manifest,
            stall_download,
            task,
        }
    }
}

struct Fixture {
    app: tauri::App<MockRuntime>,
    root: tempfile::TempDir,
    executable: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("Fixture.app/Contents/MacOS/fixture");
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, OLD).unwrap();
        let mut context = mock_context(noop_assets());
        context.config_mut().identifier = "com.wenlan.updater-fixture".into();
        context.config_mut().plugins.0.insert(
            "updater".into(),
            serde_json::json!({
                "dangerousInsecureTransportProtocol": true, "pubkey": PUBLIC_KEY.trim()
            }),
        );
        let app = mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        Self {
            app,
            root,
            executable,
        }
    }
    fn updater(&self, server: &Server) -> tauri_plugin_updater::Updater {
        // The plugin's install destination comes from this explicit executable.
        // Never use current_exe(): only our temp directory may be replaced.
        assert!(self
            .executable
            .canonicalize()
            .unwrap()
            .starts_with(self.root.path().canonicalize().unwrap()));
        configure_network(self.app.updater_builder())
            .no_proxy()
            .executable_path(&self.executable)
            .endpoints(vec![server.endpoint.parse().unwrap()])
            .unwrap()
            .build()
            .unwrap()
    }
    fn assert_bytes(&self, expected: &[u8]) {
        assert_eq!(std::fs::read(&self.executable).unwrap(), expected);
    }
}

#[tokio::test]
async fn detects_a_release_without_restarting_and_installs_signed_fixture() {
    let server = Server::start(false).await;
    let fixture = Fixture::new();
    let updater = fixture.updater(&server);
    assert!(updater.check().await.unwrap().is_none());
    server.published.store(true, Ordering::SeqCst);
    let mut update = updater.check().await.unwrap().unwrap();
    assert_eq!(update.version, "99.0.0");
    fixture.assert_bytes(OLD);
    configure_download(&mut update);
    let bytes = Arc::new(AtomicUsize::new(0));
    let progress = bytes.clone();
    update
        .download_and_install(
            move |n, _| {
                progress.fetch_add(n, Ordering::SeqCst);
            },
            || {},
        )
        .await
        .unwrap();
    assert_eq!(bytes.load(Ordering::SeqCst), ARCHIVE.len());
    fixture.assert_bytes(NEW);
}

#[tokio::test]
async fn rejects_tampered_download_without_replacing_the_fixture() {
    let server = Server::start(true).await;
    server.published.store(true, Ordering::SeqCst);
    let fixture = Fixture::new();
    let mut update = fixture.updater(&server).check().await.unwrap().unwrap();
    configure_download(&mut update);
    assert!(update.download_and_install(|_, _| {}, || {}).await.is_err());
    fixture.assert_bytes(OLD);
}

#[tokio::test]
async fn stalled_manifest_times_out_and_a_later_check_succeeds() {
    let server = Server::start(false).await;
    server.stall_manifest.store(true, Ordering::SeqCst);
    let fixture = Fixture::new();
    let updater = fixture.updater(&server);
    let result = tokio::time::timeout(CHECK_TIMEOUT + Duration::from_secs(10), updater.check())
        .await
        .expect("manifest request outlived the production timeout");
    assert!(result.is_err());
    assert!(updater.check().await.unwrap().is_none());
    fixture.assert_bytes(OLD);
}

#[tokio::test]
async fn stalled_download_times_out_without_installing_and_can_retry() {
    let server = Server::start(false).await;
    server.published.store(true, Ordering::SeqCst);
    server.stall_download.store(true, Ordering::SeqCst);
    let fixture = Fixture::new();
    let mut update = fixture.updater(&server).check().await.unwrap().unwrap();
    configure_download(&mut update);
    let result = tokio::time::timeout(
        READ_TIMEOUT + Duration::from_secs(10),
        update.download_and_install(|_, _| {}, || {}),
    )
    .await
    .expect("download outlived the production read timeout");
    assert!(result.is_err());
    fixture.assert_bytes(OLD);
    server.stall_download.store(false, Ordering::SeqCst);
    update.download_and_install(|_, _| {}, || {}).await.unwrap();
    fixture.assert_bytes(NEW);
}
