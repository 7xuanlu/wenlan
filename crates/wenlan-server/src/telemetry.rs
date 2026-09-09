// SPDX-License-Identifier: Apache-2.0
//! Small, opt-in product telemetry for the headless daemon.
//!
//! Telemetry is deliberately separate from Wenlan's general configuration:
//! consent is one strict boolean in `telemetry-preferences.json`, and the
//! in-memory batch contains only bounded operation counters. There is no
//! durable event queue, retry loop, identifier, or content inspection here.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tokio::sync::watch;

pub const TELEMETRY_ENDPOINT: &str = "https://wenlan.app/api/app-events";
const TELEMETRY_PREFERENCES_FILE: &str = "telemetry-preferences.json";
const MAX_PENDING_OPERATIONS: usize = 1_000;
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);
const BATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryEvent {
    DaemonReady,
    SaveSuccess,
    SearchNonempty,
    SearchEmpty,
    WikiGenerated,
    SaveError,
    SearchError,
    WikiError,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Counters {
    #[serde(skip_serializing_if = "is_zero")]
    pub daemon_ready: u16,
    #[serde(skip_serializing_if = "is_zero")]
    pub save_success: u16,
    #[serde(skip_serializing_if = "is_zero")]
    pub search_nonempty: u16,
    #[serde(skip_serializing_if = "is_zero")]
    pub search_empty: u16,
    #[serde(skip_serializing_if = "is_zero")]
    pub wiki_generated: u16,
    #[serde(skip_serializing_if = "is_zero")]
    pub save_error: u16,
    #[serde(skip_serializing_if = "is_zero")]
    pub search_error: u16,
    #[serde(skip_serializing_if = "is_zero")]
    pub wiki_error: u16,
}

fn is_zero(value: &u16) -> bool {
    *value == 0
}

impl Counters {
    fn increment(&mut self, event: TelemetryEvent) {
        let counter = match event {
            TelemetryEvent::DaemonReady => &mut self.daemon_ready,
            TelemetryEvent::SaveSuccess => &mut self.save_success,
            TelemetryEvent::SearchNonempty => &mut self.search_nonempty,
            TelemetryEvent::SearchEmpty => &mut self.search_empty,
            TelemetryEvent::WikiGenerated => &mut self.wiki_generated,
            TelemetryEvent::SaveError => &mut self.save_error,
            TelemetryEvent::SearchError => &mut self.search_error,
            TelemetryEvent::WikiError => &mut self.wiki_error,
        };
        // The batch-wide bound ensures this is never reached above 1000, but
        // keep the field bound explicit so a future caller cannot violate the
        // wire contract by changing the pending accounting.
        *counter = (*counter)
            .saturating_add(1)
            .min(MAX_PENDING_OPERATIONS as u16);
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.daemon_ready == 0
            && self.save_success == 0
            && self.search_nonempty == 0
            && self.search_empty == 0
            && self.wiki_generated == 0
            && self.save_error == 0
            && self.search_error == 0
            && self.wiki_error == 0
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct EventBatch {
    schema_version: u8,
    app_version: String,
    platform: &'static str,
    counters: Counters,
}

#[derive(Debug, Clone, Serialize)]
pub struct TelemetryStatus {
    pub enabled: bool,
    pub available: bool,
    pub pending_operations: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TelemetryPreferences {
    enabled: bool,
}

struct Inner {
    enabled: bool,
    /// Changes on every consent transition. A batch carries the epoch it was
    /// captured under and is discarded if consent changes before it sends.
    epoch: u64,
    pending_operations: usize,
    counters: Counters,
}

/// Process-local telemetry owner. The daemon stores one `Arc<Telemetry>` in
/// `ServerState`, so every HTTP path shares exactly one bounded batch.
pub struct Telemetry {
    inner: Mutex<Inner>,
    // File writes serialize independently of the counter/status hot path.
    consent_write: Mutex<()>,
    preferences_path: Option<PathBuf>,
    endpoint: String,
    client: Option<reqwest::Client>,
    #[cfg(test)]
    available_override: Option<bool>,
    interval: std::time::Duration,
    changes: watch::Sender<()>,
    started: AtomicBool,
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::disabled()
    }
}

impl Telemetry {
    /// Inert state used by `ServerState::default()` and all router/unit tests.
    /// It cannot persist consent or emit a request.
    pub fn disabled() -> Self {
        let telemetry = Self::from_parts(None, TELEMETRY_ENDPOINT, false, BATCH_INTERVAL);
        #[cfg(test)]
        let telemetry = telemetry.with_available_override(false);
        telemetry
    }

    /// Construct the production telemetry owner from a daemon data root.
    /// Missing, malformed, or non-strict preference files are interpreted as
    /// consent OFF.
    pub fn from_data_root(data_root: impl Into<PathBuf>) -> Self {
        let data_root = data_root.into();
        let preferences_path = data_root.join(TELEMETRY_PREFERENCES_FILE);
        let enabled = load_enabled(&preferences_path);
        Self::from_parts(
            Some(preferences_path),
            TELEMETRY_ENDPOINT,
            enabled,
            BATCH_INTERVAL,
        )
    }

    /// Test-only construction seam. Production code never supplies a custom
    /// endpoint; tests use a local mock receiver and a short interval.
    #[cfg(test)]
    pub fn for_test(
        data_root: impl Into<PathBuf>,
        endpoint: impl Into<String>,
        interval: std::time::Duration,
    ) -> Self {
        let data_root = data_root.into();
        let preferences_path = data_root.join(TELEMETRY_PREFERENCES_FILE);
        let enabled = load_enabled(&preferences_path);
        Self::from_parts(Some(preferences_path), endpoint, enabled, interval)
            .with_available_override(true)
    }

    fn from_parts(
        preferences_path: Option<PathBuf>,
        endpoint: impl Into<String>,
        enabled: bool,
        interval: std::time::Duration,
    ) -> Self {
        let (changes, _) = watch::channel(());
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("Wenlan-Telemetry/1")
            .build()
            .ok();
        Self {
            consent_write: Mutex::new(()),
            inner: Mutex::new(Inner {
                enabled,
                epoch: 0,
                pending_operations: 0,
                counters: Counters::default(),
            }),
            preferences_path,
            endpoint: endpoint.into(),
            client,
            #[cfg(test)]
            available_override: None,
            interval,
            changes,
            started: AtomicBool::new(false),
        }
    }

    #[cfg(test)]
    fn with_available_override(mut self, available: bool) -> Self {
        self.available_override = Some(available);
        self
    }

    fn available(&self) -> bool {
        #[cfg(test)]
        let compile_available = self.available_override.unwrap_or(!cfg!(debug_assertions));
        #[cfg(not(test))]
        let compile_available = !cfg!(debug_assertions);
        compile_available
            && self.client.is_some()
            && std::env::var("WENLAN_TELEMETRY_DISABLED").as_deref() != Ok("1")
    }

    pub fn status(&self) -> TelemetryStatus {
        let inner = lock_unpoisoned(&self.inner);
        TelemetryStatus {
            enabled: inner.enabled,
            available: self.available(),
            pending_operations: inner.pending_operations,
        }
    }

    /// Enable only after persistence. Revoke the live gate before persisting
    /// OFF, even if the filesystem write subsequently fails.
    pub fn set_enabled(&self, enabled: bool) -> std::io::Result<()> {
        let _write = lock_unpoisoned(&self.consent_write);
        if !enabled {
            let mut inner = lock_unpoisoned(&self.inner);
            if inner.enabled {
                inner.epoch = inner.epoch.wrapping_add(1);
            }
            inner.enabled = false;
            inner.pending_operations = 0;
            inner.counters.clear();
            let _ = self.changes.send(());
        }
        let persist_result = self
            .preferences_path
            .as_deref()
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "telemetry is not configured for this server state",
                )
            })
            .and_then(|path| persist_enabled(path, enabled));
        // Enabling is fail-closed: no in-memory consent without a durable
        // preference. Disabling is fail-closed too, but must clear the live
        // batch even if its preference write cannot complete.
        if !enabled || persist_result.is_err() {
            return persist_result;
        }
        let mut inner = lock_unpoisoned(&self.inner);
        if inner.enabled != enabled {
            inner.epoch = inner.epoch.wrapping_add(1);
        }
        inner.enabled = enabled;
        let _ = self.changes.send(());
        persist_result
    }

    /// Record one anonymous operation counter without waiting or inspecting
    /// request content. Disabled/unavailable telemetry is a no-op.
    pub fn record(&self, event: TelemetryEvent) {
        if !self.available() {
            return;
        }
        let mut inner = lock_unpoisoned(&self.inner);
        if !inner.enabled || inner.pending_operations >= MAX_PENDING_OPERATIONS {
            return;
        }
        inner.counters.increment(event);
        inner.pending_operations += 1;
    }

    /// Start the hourly loop once. The initial interval tick is consumed so
    /// opting in never causes an immediate flush.
    pub fn start(self: &Arc<Self>, shutdown: crate::lifecycle::ShutdownHandle) {
        if !self.available() || self.started.swap(true, Ordering::AcqRel) {
            return;
        }
        if self.status().enabled {
            self.record(TelemetryEvent::DaemonReady);
        }
        let telemetry = Arc::clone(self);
        let mut shutdown = shutdown.subscribe();
        let mut changes = telemetry.changes.subscribe();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(telemetry.interval);
            interval.tick().await;
            loop {
                tokio::select! {
                    _ = interval.tick() => telemetry.flush(&mut changes, &mut shutdown).await,
                    changed = changes.changed() => {
                        if changed.is_err() { return; }
                    }
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() { return; }
                    }
                }
            }
        });
    }

    async fn flush(&self, changes: &mut watch::Receiver<()>, shutdown: &mut watch::Receiver<bool>) {
        let (epoch, payload) = {
            let mut inner = lock_unpoisoned(&self.inner);
            if !inner.enabled || inner.pending_operations == 0 || !self.available() {
                return;
            }
            let epoch = inner.epoch;
            let counters = std::mem::take(&mut inner.counters);
            inner.pending_operations = 0;
            (
                epoch,
                EventBatch {
                    schema_version: 1,
                    app_version: normalized_app_version(),
                    platform: platform(),
                    counters,
                },
            )
        };

        // Consent can be revoked between taking the batch and starting the
        // request. A changed watch receiver cancels this in-flight attempt;
        // the batch is intentionally dropped because there is no retry queue.
        if !self.enabled_at_epoch(epoch) {
            return;
        }
        let Some(client) = self.client.as_ref() else {
            return;
        };
        let request = client.post(&self.endpoint).json(&payload).send();
        let result = tokio::select! {
            biased;
            changed = changes.changed() => {
                let _ = changed;
                None
            }
            changed = shutdown.changed() => {
                let _ = changed;
                None
            }
            result = request => Some(result),
        };
        if let Some(Err(error)) = result {
            tracing::debug!("telemetry batch dropped: {error}");
        }
    }

    fn enabled_at_epoch(&self, epoch: u64) -> bool {
        let inner = lock_unpoisoned(&self.inner);
        inner.enabled && inner.epoch == epoch && self.available()
    }

    #[cfg(test)]
    fn pending_counters(&self) -> Counters {
        lock_unpoisoned(&self.inner).counters.clone()
    }
}

fn load_enabled(path: &Path) -> bool {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<TelemetryPreferences>(&bytes).ok())
        .map(|prefs| prefs.enabled)
        .unwrap_or(false)
}

fn persist_enabled(path: &Path, enabled: bool) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec(&TelemetryPreferences { enabled }).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("serialize telemetry preferences: {error}"),
        )
    })?;
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, payload)?;
    // `rename` replaces an existing path on Unix but not on Windows. Consent
    // is fail-closed, so a brief missing file during the Windows replacement
    // is safer than leaving an old value after a successful PUT.
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(temporary, path)
}

fn normalized_app_version() -> String {
    env!("CARGO_PKG_VERSION")
        .split(['-', '+'])
        .next()
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .to_string()
}

const fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn consent_persistence_lock_does_not_block_operation_counters() {
        let dir = tempdir().unwrap();
        let telemetry =
            Telemetry::for_test(dir.path(), "http://127.0.0.1:1/events", BATCH_INTERVAL);
        telemetry.set_enabled(true).unwrap();
        let _write = lock_unpoisoned(&telemetry.consent_write);
        // A slow consent filesystem operation owns this lock, not `inner`.
        telemetry.record(TelemetryEvent::SaveSuccess);
        assert_eq!(telemetry.status().pending_operations, 1);
    }

    #[tokio::test]
    async fn disabling_cancels_a_hanging_inflight_batch_without_retry() {
        use tokio::io::AsyncReadExt;
        let dir = tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/events", listener.local_addr().unwrap());
        let (accepted, received) = tokio::sync::oneshot::channel();
        let receiver = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 2048];
            assert!(stream.read(&mut bytes).await.unwrap() > 0);
            let _ = accepted.send(());
            std::future::pending::<()>().await;
        });
        let telemetry = Arc::new(Telemetry::for_test(dir.path(), endpoint, BATCH_INTERVAL));
        telemetry.set_enabled(true).unwrap();
        telemetry.record(TelemetryEvent::SaveSuccess);
        let mut changes = telemetry.changes.subscribe();
        let (_shutdown_sender, mut shutdown) = watch::channel(false);
        let sender = telemetry.clone();
        let flushing = tokio::spawn(async move { sender.flush(&mut changes, &mut shutdown).await });
        tokio::time::timeout(std::time::Duration::from_secs(2), received)
            .await
            .unwrap()
            .unwrap();
        telemetry.set_enabled(false).unwrap();
        tokio::time::timeout(std::time::Duration::from_millis(500), flushing)
            .await
            .unwrap()
            .unwrap();
        assert!(!telemetry.status().enabled);
        assert_eq!(telemetry.status().pending_operations, 0);
        receiver.abort();
    }
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn receive_request(listener: tokio::net::TcpListener) -> serde_json::Value {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let body_start = loop {
            let mut chunk = [0_u8; 4096];
            let read =
                tokio::time::timeout(std::time::Duration::from_secs(1), stream.read(&mut chunk))
                    .await
                    .unwrap()
                    .unwrap();
            assert!(read > 0, "mock receiver saw an incomplete HTTP request");
            bytes.extend_from_slice(&chunk[..read]);

            let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find_map(|(name, value)| {
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_length {
                break header_end + 4;
            }
        };

        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        serde_json::from_slice(&bytes[body_start..]).unwrap()
    }

    #[test]
    fn malformed_or_extra_preference_is_off() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(TELEMETRY_PREFERENCES_FILE);
        std::fs::write(&path, br#"{"enabled":true,"extra":1}"#).unwrap();
        assert!(!load_enabled(&path));
        std::fs::write(&path, br#"{"enabled":1}"#).unwrap();
        assert!(!load_enabled(&path));
        std::fs::write(&path, br#"{}"#).unwrap();
        assert!(!load_enabled(&path));
    }

    #[test]
    fn exact_wire_batch_has_only_contract_fields() {
        let dir = tempdir().unwrap();
        let telemetry =
            Telemetry::for_test(dir.path(), "http://127.0.0.1:1/events", BATCH_INTERVAL);
        telemetry.set_enabled(true).unwrap();
        telemetry.record(TelemetryEvent::SaveSuccess);
        telemetry.record(TelemetryEvent::SearchEmpty);
        let counters = telemetry.pending_counters();
        let batch = EventBatch {
            schema_version: 1,
            app_version: normalized_app_version(),
            platform: platform(),
            counters,
        };
        let value = serde_json::to_value(batch).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["counters"]["save_success"], 1);
        assert_eq!(value["counters"]["search_empty"], 1);
        assert!(value["counters"].get("daemon_ready").is_none());
        assert_eq!(value.as_object().unwrap().len(), 4);
        assert_eq!(value["app_version"], normalized_app_version());
        assert_eq!(value["platform"], platform());
    }

    #[test]
    fn disable_clears_pending_and_persists() {
        let dir = tempdir().unwrap();
        let telemetry =
            Telemetry::for_test(dir.path(), "http://127.0.0.1:1/events", BATCH_INTERVAL);
        telemetry.set_enabled(true).unwrap();
        telemetry.record(TelemetryEvent::SaveSuccess);
        assert_eq!(telemetry.status().pending_operations, 1);
        telemetry.set_enabled(false).unwrap();
        assert_eq!(telemetry.status().pending_operations, 0);
        assert!(!load_enabled(&dir.path().join(TELEMETRY_PREFERENCES_FILE)));
    }

    #[test]
    fn consent_survives_restart_and_missing_defaults_off() {
        let dir = tempdir().unwrap();
        assert!(
            !Telemetry::for_test(dir.path(), "http://127.0.0.1:1/events", BATCH_INTERVAL,)
                .status()
                .enabled
        );

        let first = Telemetry::for_test(dir.path(), "http://127.0.0.1:1/events", BATCH_INTERVAL);
        first.set_enabled(true).unwrap();
        let restarted =
            Telemetry::for_test(dir.path(), "http://127.0.0.1:1/events", BATCH_INTERVAL);
        assert!(restarted.status().enabled);
    }

    #[test]
    fn pending_is_bounded() {
        let dir = tempdir().unwrap();
        let telemetry =
            Telemetry::for_test(dir.path(), "http://127.0.0.1:1/events", BATCH_INTERVAL);
        telemetry.set_enabled(true).unwrap();
        for _ in 0..(MAX_PENDING_OPERATIONS + 50) {
            telemetry.record(TelemetryEvent::SaveSuccess);
        }
        let status = telemetry.status();
        assert_eq!(status.pending_operations, MAX_PENDING_OPERATIONS);
        assert_eq!(
            telemetry.pending_counters().save_success,
            MAX_PENDING_OPERATIONS as u16
        );
    }

    #[tokio::test]
    async fn no_request_or_counter_before_consent() {
        let dir = tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/events", listener.local_addr().unwrap());
        let telemetry = Telemetry::for_test(dir.path(), endpoint, BATCH_INTERVAL);
        telemetry.record(TelemetryEvent::SaveSuccess);
        assert_eq!(telemetry.status().pending_operations, 0);

        let (_changes_sender, mut changes) = watch::channel(());
        let (_shutdown_sender, mut shutdown) = watch::channel(false);
        telemetry.flush(&mut changes, &mut shutdown).await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "consent-off telemetry must not open a request"
        );
    }

    #[tokio::test]
    async fn local_receiver_captures_exact_wire_batch() {
        let dir = tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/events", listener.local_addr().unwrap());
        let receiver = tokio::spawn(receive_request(listener));
        let telemetry = Telemetry::for_test(dir.path(), endpoint, BATCH_INTERVAL);
        telemetry.set_enabled(true).unwrap();
        telemetry.record(TelemetryEvent::SaveSuccess);
        telemetry.record(TelemetryEvent::SearchEmpty);

        let (_changes_sender, mut changes) = watch::channel(());
        let (_shutdown_sender, mut shutdown) = watch::channel(false);
        telemetry.flush(&mut changes, &mut shutdown).await;
        let value = tokio::time::timeout(std::time::Duration::from_secs(1), receiver)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(value.as_object().unwrap().len(), 4);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["app_version"], normalized_app_version());
        assert_eq!(value["platform"], platform());
        assert_eq!(
            value["counters"],
            serde_json::json!({"save_success": 1, "search_empty": 1})
        );
    }

    #[tokio::test]
    async fn disable_clears_and_prevents_a_later_request() {
        let dir = tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/events", listener.local_addr().unwrap());
        let telemetry = Telemetry::for_test(dir.path(), endpoint, BATCH_INTERVAL);
        telemetry.set_enabled(true).unwrap();
        telemetry.record(TelemetryEvent::SaveSuccess);
        telemetry.set_enabled(false).unwrap();
        assert!(!telemetry.status().enabled);
        assert_eq!(telemetry.status().pending_operations, 0);

        let (_changes_sender, mut changes) = watch::channel(());
        let (_shutdown_sender, mut shutdown) = watch::channel(false);
        telemetry.flush(&mut changes, &mut shutdown).await;
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "disabled telemetry must not send a cleared batch"
        );
    }

    #[test]
    fn failed_disable_persist_still_turns_off_and_clears_live_queue() {
        let dir = tempdir().unwrap();
        let mut telemetry =
            Telemetry::for_test(dir.path(), "http://127.0.0.1:1/events", BATCH_INTERVAL);
        telemetry.set_enabled(true).unwrap();
        telemetry.record(TelemetryEvent::SaveSuccess);
        let blocked_path = dir.path().join("blocked-preferences");
        std::fs::create_dir(&blocked_path).unwrap();
        telemetry.preferences_path = Some(blocked_path);

        assert!(telemetry.set_enabled(false).is_err());
        assert!(!telemetry.status().enabled);
        assert_eq!(telemetry.status().pending_operations, 0);
        assert!(telemetry.pending_counters().is_empty());
    }

    #[tokio::test]
    async fn failed_network_batch_is_nonblocking_and_not_retried() {
        let dir = tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/events", listener.local_addr().unwrap());
        drop(listener);

        let telemetry = Telemetry::for_test(dir.path(), endpoint, BATCH_INTERVAL);
        telemetry.set_enabled(true).unwrap();
        telemetry.record(TelemetryEvent::SaveSuccess);
        let (_changes_sender, mut changes) = watch::channel(());
        let (_shutdown_sender, mut shutdown) = watch::channel(false);

        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            telemetry.flush(&mut changes, &mut shutdown),
        )
        .await
        .expect("connection failure must not block the telemetry loop");
        assert_eq!(telemetry.status().pending_operations, 0);

        // There is no durable queue: a second tick has no payload to retry.
        telemetry.flush(&mut changes, &mut shutdown).await;
        assert_eq!(telemetry.status().pending_operations, 0);
    }
}
