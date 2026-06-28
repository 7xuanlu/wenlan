// SPDX-License-Identifier: AGPL-3.0-only
//! Tauri command surface — thin HTTP client that proxies data operations
//! to the Wenlan daemon at http://127.0.0.1:7878.
//!
//! UI-only commands (window positioning, permissions, shortcuts) remain local.
//! Config-only commands (skip lists, setup wizard, etc.) remain local.
//! All data/DB commands proxy through `state.client`.

use crate::activity;
use crate::config;
use crate::sources::SourceStatus;
use crate::state::{AppState, IndexStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use wenlan_types::requests;
use wenlan_types::responses;
use wenlan_types::*;

type State = Arc<RwLock<AppState>>;
type WatcherState = Arc<tokio::sync::Mutex<Option<crate::indexer::FileWatcher>>>;

// ── Request types (kept for Tauri IPC deserialization) ─────────────────

#[derive(Debug, Deserialize)]
pub struct QuickCaptureRequest {
    pub title: Option<String>,
    pub content: String,
    pub tags: Option<Vec<String>>,
    pub memory_type: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StoreMemoryRequest {
    pub content: String,
    pub memory_type: Option<String>,
    pub domain: Option<String>,
    pub source_agent: Option<String>,
    pub title: Option<String>,
    pub tags: Option<Vec<String>>,
    pub confidence: Option<f32>,
    pub supersedes: Option<String>,
    pub structured_fields: Option<serde_json::Value>,
    pub retrieval_cue: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMemoryRequest {
    pub query: String,
    pub limit: Option<usize>,
    pub memory_type: Option<String>,
    pub domain: Option<String>,
    pub source_agent: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListMemoriesRequest {
    pub memory_type: Option<String>,
    pub domain: Option<String>,
    pub limit: Option<usize>,
}

// ── Response types ────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct StoreMemoryResponse {
    pub source_id: String,
    pub warnings: Vec<String>,
    /// Background-enrichment state from the daemon — `"pending"` when
    /// classify + extract will run asynchronously, `"not_needed"` when
    /// no LLM is available. The frontend uses this to drive live-update
    /// UI (invalidate the stored-memory query once background work lands).
    /// Defaulted so older daemon responses still deserialize cleanly.
    #[serde(default)]
    pub enrichment: String,
    /// Prose nudge for callers — safe to show to the user. Empty when no
    /// enrichment will run.
    #[serde(default)]
    pub hint: String,
    /// Protected memories now flagged for human revision by the daemon.
    #[serde(default)]
    pub triggered_revisions: Vec<String>,
    /// Protected memories auto-accepted by a full-trust agent write.
    #[serde(default)]
    pub auto_superseded: Vec<String>,
}

// ── Window / UI commands (kept as-is) ─────────────────────────────────

#[tauri::command]
pub fn set_traffic_lights_visible(window: tauri::Window, visible: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    #[allow(deprecated)]
    {
        use cocoa::appkit::{NSWindow, NSWindowButton};
        use raw_window_handle::HasWindowHandle;

        let raw_handle = window.window_handle().map_err(|e| e.to_string())?;
        if let raw_window_handle::RawWindowHandle::AppKit(appkit) = raw_handle.as_raw() {
            let ns_view = appkit.ns_view.as_ptr() as cocoa::base::id;
            unsafe {
                let ns_win: cocoa::base::id = objc::msg_send![ns_view, window];
                for button in &[
                    NSWindowButton::NSWindowCloseButton,
                    NSWindowButton::NSWindowMiniaturizeButton,
                    NSWindowButton::NSWindowZoomButton,
                ] {
                    let btn: cocoa::base::id = ns_win.standardWindowButton_(*button);
                    if btn != cocoa::base::nil {
                        let _: () = objc::msg_send![btn, setHidden:!visible];
                    }
                }
            }
        }
    }
    Ok(())
}

/// Hide quick-capture and prevent macOS from auto-activating the main window.
/// Called from the quick-capture webview on Esc / Enter save.
#[tauri::command]
pub async fn dismiss_quick_capture(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    if let Some(qc) = app.get_webview_window("quick-capture") {
        // Use orderOut instead of hide() to remove the window without
        // triggering macOS window promotion (which would show main).
        #[cfg(target_os = "macos")]
        #[allow(deprecated)]
        {
            use cocoa::base::id;
            use raw_window_handle::HasWindowHandle;
            if let Ok(raw_handle) = qc.window_handle() {
                if let raw_window_handle::RawWindowHandle::AppKit(appkit) = raw_handle.as_raw() {
                    let ns_view = appkit.ns_view.as_ptr() as id;
                    unsafe {
                        let ns_win: id = objc::msg_send![ns_view, window];
                        let _: () = objc::msg_send![ns_win, orderOut: ns_win];
                    }
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = qc.hide();
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn position_quick_capture(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let win = app
        .get_webview_window("quick-capture")
        .ok_or("quick-capture window not found")?;

    #[cfg(target_os = "macos")]
    #[allow(deprecated)]
    {
        use cocoa::base::id;
        use cocoa::foundation::NSRect;
        use raw_window_handle::HasWindowHandle;
        use tauri::{LogicalPosition, LogicalSize};

        let raw_handle = win.window_handle().map_err(|e| e.to_string())?;
        if let raw_window_handle::RawWindowHandle::AppKit(appkit) = raw_handle.as_raw() {
            let ns_view = appkit.ns_view.as_ptr() as id;

            let (visible, screen_h) = unsafe {
                let ns_win: id = objc::msg_send![ns_view, window];
                if ns_win.is_null() {
                    return Err("NSWindow not attached".into());
                }
                let screen: id = objc::msg_send![ns_win, screen];
                if screen.is_null() {
                    return Err("NSScreen not available".into());
                }
                let visible: NSRect = objc::msg_send![screen, visibleFrame];
                let frame: NSRect = objc::msg_send![screen, frame];
                (visible, frame.size.height)
            };

            let width = 400.0;
            let height = 160.0;
            let padding = 16.0;

            win.set_size(LogicalSize::new(width, height))
                .map_err(|e| e.to_string())?;

            let x = visible.origin.x + visible.size.width - width - padding;
            let y = screen_h - visible.origin.y - padding - height;

            log::debug!("[qc-pos] visible=({:.0},{:.0} {:.0}x{:.0}) screen_h={:.0} → size=({:.0},{:.0}) pos=({:.0},{:.0})",
                visible.origin.x, visible.origin.y, visible.size.width, visible.size.height,
                screen_h, width, height, x, y);

            win.set_position(LogicalPosition::new(x, y))
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

// ── Permission commands (kept as-is) ──────────────────────────────────

#[tauri::command]
pub fn check_screen_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGPreflightScreenCaptureAccess() -> bool;
        }
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[tauri::command]
pub fn request_screen_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGRequestScreenCaptureAccess() -> bool;
        }
        unsafe { CGRequestScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

// ── Config-only commands (kept as-is) ─────────────────────────────────

#[tauri::command]
pub async fn get_clipboard_enabled(state: tauri::State<'_, State>) -> Result<bool, String> {
    let state = state.read().await;
    Ok(state.clipboard_enabled)
}

#[tauri::command]
pub async fn set_clipboard_enabled(
    state: tauri::State<'_, State>,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut app_state = state.write().await;
        app_state.clipboard_enabled = enabled;
    }
    save_current_config(&state)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn get_api_key() -> Result<Option<String>, String> {
    let config = config::load_config();
    Ok(config.anthropic_api_key.map(|key| {
        let chars: Vec<char> = key.chars().collect();
        if chars.len() > 12 {
            let prefix: String = chars[..8].iter().collect();
            let suffix: String = chars[chars.len() - 4..].iter().collect();
            format!("{}...{}", prefix, suffix)
        } else {
            "***".to_string()
        }
    }))
}

#[tauri::command]
pub async fn set_api_key(state: tauri::State<'_, State>, key: String) -> Result<(), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    if key.trim().is_empty() {
        client
            .delete_path::<serde_json::Value>("/api/setup/anthropic-key")
            .await?;
    } else {
        let body = serde_json::json!({ "api_key": key });
        client
            .put_json::<_, serde_json::Value>("/api/setup/anthropic-key", &body)
            .await?;
    }
    log::info!("[settings] API key updated");
    Ok(())
}

// Phase 5-D Phase 4: route config reads/writes through the daemon so the
// daemon's in-memory cache stays consistent with on-disk config.json.
// Direct `crate::config::load_config()` calls here would let app and daemon
// drift (the daemon caches at startup; sensors keep using stale skip lists
// after a local-only write).

#[tauri::command]
pub async fn get_skip_apps(state: tauri::State<'_, State>) -> Result<Vec<String>, String> {
    let s = state.read().await;
    s.client.get_skip_apps().await
}

#[tauri::command]
pub async fn set_skip_apps(
    state: tauri::State<'_, State>,
    apps: Vec<String>,
) -> Result<(), String> {
    let s = state.read().await;
    s.client.set_skip_apps(apps).await
}

#[tauri::command]
pub async fn get_skip_title_patterns(
    state: tauri::State<'_, State>,
) -> Result<Vec<String>, String> {
    let s = state.read().await;
    s.client.get_skip_title_patterns().await
}

#[tauri::command]
pub async fn set_skip_title_patterns(
    state: tauri::State<'_, State>,
    patterns: Vec<String>,
) -> Result<(), String> {
    let s = state.read().await;
    s.client.set_skip_title_patterns(patterns).await
}

#[tauri::command]
pub async fn get_private_browsing_detection() -> Result<bool, String> {
    let cfg = crate::config::load_config();
    Ok(cfg.private_browsing_detection)
}

#[tauri::command]
pub async fn set_private_browsing_detection(enabled: bool) -> Result<(), String> {
    let mut cfg = crate::config::load_config();
    cfg.private_browsing_detection = enabled;
    crate::config::save_config(&cfg).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_setup_status(
    state: tauri::State<'_, State>,
) -> Result<crate::api::SetupStatusResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.get_setup_status().await
}

#[tauri::command]
pub async fn get_setup_completed(state: tauri::State<'_, State>) -> Result<bool, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    Ok(client.get_setup_status().await?.setup_completed)
}

#[tauri::command]
pub async fn set_setup_completed(
    state: tauri::State<'_, State>,
    completed: bool,
) -> Result<(), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.set_setup_completed(completed).await
}

#[tauri::command]
pub async fn should_show_wizard(state: tauri::State<'_, State>) -> Result<bool, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    Ok(!client.get_setup_status().await?.setup_completed)
}

#[tauri::command]
pub async fn detect_mcp_clients_cmd() -> Result<Vec<crate::mcp_config::McpClient>, String> {
    Ok(crate::mcp_config::detect_mcp_clients())
}

#[tauri::command]
pub async fn write_mcp_config(client_type: String) -> Result<(), String> {
    let config_path = crate::mcp_config::client_config_path(&client_type)
        .ok_or(format!("Unknown client type: {}", client_type))?;
    let is_claude_code = client_type == "claude_code";
    crate::mcp_config::write_wenlan_entry(&config_path, is_claude_code).map_err(|e| e.to_string())
}

/// Returns the current `wenlan` MCP server entry (command + args) that Wenlan
/// uses when writing client configs. Prefers a local binary in dev, falls back
/// to `npx -y wenlan-mcp` otherwise. The frontend uses this to build a
/// copy-pasteable manual-setup JSON snippet with real values instead of
/// `/path/to/wenlan-mcp` placeholder text.
#[tauri::command]
pub async fn get_wenlan_mcp_entry() -> Result<serde_json::Value, String> {
    Ok(crate::mcp_config::wenlan_mcp_entry())
}

#[tauri::command]
pub async fn get_screen_capture_enabled(state: tauri::State<'_, State>) -> Result<bool, String> {
    Ok(state.read().await.screen_capture_enabled)
}

#[tauri::command]
pub async fn set_screen_capture_enabled(
    state: tauri::State<'_, State>,
    enabled: bool,
) -> Result<(), String> {
    {
        let mut app_state = state.write().await;
        app_state.screen_capture_enabled = enabled;
    }
    let mut config = crate::config::load_config();
    config.screen_capture_enabled = enabled;
    crate::config::save_config(&config).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Activity commands (file-based, local) ─────────────────────────────

#[tauri::command]
pub async fn list_activities(
    state: tauri::State<'_, State>,
) -> Result<Vec<activity::ActivitySummary>, String> {
    let s = state.read().await;
    Ok(s.list_activity_summaries())
}

#[tauri::command]
pub async fn rebuild_activities(state: tauri::State<'_, State>) -> Result<usize, String> {
    // In thin-client mode, we cannot scan the DB for timestamps.
    // Keep the file-based activity rebuild from completed_activities.
    let s = state.read().await;
    Ok(s.completed_activities.len())
}

#[tauri::command]
pub async fn get_capture_stats(
    state: tauri::State<'_, State>,
) -> Result<HashMap<String, u64>, String> {
    let s = state.read().await;
    s.client.get_capture_stats().await
}

// ── Trigger/sensor commands ───────────────────────────────────────────

#[tauri::command]
pub async fn trigger_manual_capture(state: tauri::State<'_, State>) -> Result<(), String> {
    let s = state.read().await;
    if let Some(ref tx) = s.trigger_tx {
        tx.send(crate::trigger::types::TriggerEvent::ManualHotkey)
            .await
            .map_err(|e| format!("{}", e))?;
    }
    Ok(())
}

// ── Remote access commands (kept as-is) ───────────────────────────────

#[tauri::command]
pub async fn toggle_remote_access(
    _state: tauri::State<'_, State>,
    app_handle: tauri::AppHandle,
    enabled: bool,
) -> Result<crate::remote_access::RemoteAccessStatus, String> {
    if enabled {
        let mut existing = config::load_config();
        existing.remote_access_enabled = true;
        config::save_config(&existing).map_err(|e| e.to_string())?;

        let handle = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            crate::remote_access::toggle_on(handle, false).await;
        });

        Ok(crate::remote_access::RemoteAccessStatus::Starting)
    } else {
        let mut existing = config::load_config();
        existing.remote_access_enabled = false;
        config::save_config(&existing).map_err(|e| e.to_string())?;

        crate::remote_access::toggle_off(&app_handle).await;
        Ok(crate::remote_access::RemoteAccessStatus::Off)
    }
}

#[tauri::command]
pub async fn get_remote_access_status(
    state: tauri::State<'_, State>,
) -> Result<crate::remote_access::RemoteAccessStatus, String> {
    let app_state = state.read().await;
    let ra = app_state.remote_access.lock().await;
    Ok(ra.status.clone())
}

#[derive(serde::Serialize)]
pub struct RemoteConnectionTest {
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn test_remote_mcp_connection(
    state: tauri::State<'_, State>,
) -> Result<RemoteConnectionTest, String> {
    // Snapshot out of the lock, then drop the guard.
    //
    // Prefer `relay_url` over `tunnel_url`. The relay domain
    // (origin-relay.originmemory.workers.dev) always resolves via system DNS,
    // while fresh `*.trycloudflare.com` tunnel subdomains can hit ISP DNS
    // cache NXDOMAIN for several minutes — a known Cloudflare quick-tunnel
    // issue. The relay URL also reflects what the user actually hands to
    // Claude.ai / ChatGPT, so probing it is semantically correct.
    let (probe_url, is_relay): (Option<String>, bool) = {
        let app_state = state.read().await;
        let ra = app_state.remote_access.lock().await;
        match &ra.status {
            crate::remote_access::RemoteAccessStatus::Connected {
                tunnel_url,
                relay_url,
                ..
            } => match relay_url {
                Some(url) => (Some(url.clone()), true),
                None => (Some(tunnel_url.clone()), false),
            },
            _ => (None, false),
        }
    };
    let Some(url) = probe_url else {
        return Ok(RemoteConnectionTest {
            ok: false,
            latency_ms: None,
            error: Some("Remote Access not connected".to_string()),
        });
    };
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Ok(RemoteConnectionTest {
                ok: false,
                latency_ms: None,
                error: Some(format!("http client: {}", e)),
            });
        }
    };
    let start = std::time::Instant::now();
    // Raw tunnel URL: probe `/health` (wenlan-mcp serves it; expect 2xx).
    // Relay URL: probe the URL directly — any HTTP response (even 4xx from
    // method-not-allowed on GET /mcp) proves DNS + TLS + worker reachable;
    // only 5xx / connection errors indicate a real problem.
    let probe = if is_relay {
        url.clone()
    } else {
        format!("{}/health", url.trim_end_matches('/'))
    };
    match client.get(&probe).send().await {
        Ok(resp) => {
            let status = resp.status();
            let latency = Some(start.elapsed().as_millis() as u64);
            let reachable = if is_relay {
                !status.is_server_error()
            } else {
                status.is_success()
            };
            if reachable {
                Ok(RemoteConnectionTest {
                    ok: true,
                    latency_ms: latency,
                    error: None,
                })
            } else {
                Ok(RemoteConnectionTest {
                    ok: false,
                    latency_ms: latency,
                    error: Some(format!("HTTP {}", status)),
                })
            }
        }
        Err(e) => Ok(RemoteConnectionTest {
            ok: false,
            latency_ms: None,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn rotate_remote_token(
    _state: tauri::State<'_, State>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    crate::remote_access::rotate_token(&app_handle).await
}

// ── File / open commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn open_file(path: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Failed to open file: {}", e))?;
    Ok(())
}

// ── Index / watch path / source commands (local + config) ─────────────

#[tauri::command]
pub async fn get_index_status(state: tauri::State<'_, State>) -> Result<IndexStatus, String> {
    let (client, local) = {
        let state = state.read().await;
        (state.client.clone(), state.index_status.clone())
    };
    let daemon = client.status().await?;
    Ok(merge_daemon_status(local, daemon))
}

fn merge_daemon_status(mut local: IndexStatus, daemon: responses::StatusResponse) -> IndexStatus {
    local.files_indexed = daemon.files_indexed;
    local.sources_connected = daemon.sources_connected;
    local.reranker = daemon.reranker;
    local.reranker_light = daemon.reranker_light;
    local.reranker_mode = daemon.reranker_mode;
    local
}

#[tauri::command]
pub async fn list_watch_paths(state: tauri::State<'_, State>) -> Result<Vec<String>, String> {
    let state = state.read().await;
    Ok(state
        .watch_paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect())
}

#[tauri::command]
pub async fn add_watch_path(
    state: tauri::State<'_, State>,
    watcher: tauri::State<'_, WatcherState>,
    path: String,
) -> Result<(), String> {
    let path = PathBuf::from(&path);
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("Path is not a directory: {}", path.display()));
    }

    {
        let mut app_state = state.write().await;
        if let Some(source) = app_state.sources.get_mut("local_files") {
            if let Some(local) = source
                .as_any_mut()
                .downcast_mut::<crate::sources::local_files::LocalFilesSource>()
            {
                local.add_watch_path(path.clone());
            }
        }
        if !app_state.watch_paths.contains(&path) {
            app_state.watch_paths.push(path.clone());
        }
    }

    let mut watcher_guard = watcher.lock().await;
    if watcher_guard.is_none() {
        let state_arc = state.inner().clone();
        *watcher_guard =
            Some(crate::indexer::create_file_watcher(state_arc).map_err(|e| e.to_string())?);
    }
    if let Some(w) = watcher_guard.as_mut() {
        crate::indexer::watch_path(w, &path).map_err(|e| e.to_string())?;
    }

    // Persist as a Source entry in config
    {
        let mut cfg = config::load_config();
        if !cfg.sources.iter().any(|s| s.path == path) {
            let dirname = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "dir".to_string());
            let slug = crate::sources::obsidian::slugify(&dirname);
            let id = format!("directory-{}", slug);
            cfg.sources.push(crate::sources::Source {
                id,
                source_type: crate::sources::SourceType::Directory,
                path: path.clone(),
                status: crate::sources::SyncStatus::Active,
                last_sync: None,
                file_count: 0,
                memory_count: 0,
                last_sync_errors: 0,
                last_sync_error_detail: None,
            });
            config::save_config(&cfg).map_err(|e| e.to_string())?;
        }
    }

    save_current_config(&state)
        .await
        .map_err(|e| e.to_string())?;

    // Trigger initial index
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::indexer::sync_source("local_files", &state_inner).await {
            log::error!("Initial index after add_watch_path failed: {}", e);
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn remove_watch_path(
    state: tauri::State<'_, State>,
    watcher: tauri::State<'_, WatcherState>,
    path: String,
) -> Result<(), String> {
    let path = PathBuf::from(&path);

    {
        let mut app_state = state.write().await;
        if let Some(source) = app_state.sources.get_mut("local_files") {
            if let Some(local) = source
                .as_any_mut()
                .downcast_mut::<crate::sources::local_files::LocalFilesSource>()
            {
                local.remove_watch_path(&path);
            }
        }
        app_state.watch_paths.retain(|p| p != &path);
    }

    let mut watcher_guard = watcher.lock().await;
    if let Some(w) = watcher_guard.as_mut() {
        let _ = w.unwatch(&path);
    }

    // Remove from config.sources
    {
        let mut cfg = config::load_config();
        let before = cfg.sources.len();
        cfg.sources.retain(|s| s.path != path);
        if cfg.sources.len() != before {
            config::save_config(&cfg).map_err(|e| e.to_string())?;
        }
    }

    save_current_config(&state)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn reindex(state: tauri::State<'_, State>) -> Result<(), String> {
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::indexer::sync_source("local_files", &state_inner).await {
            log::error!("Reindex failed: {}", e);
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn connect_source(
    state: tauri::State<'_, State>,
    source_name: String,
) -> Result<(), String> {
    {
        let mut s = state.write().await;
        let source = s
            .sources
            .get_mut(&source_name)
            .ok_or_else(|| format!("Unknown source: {}", source_name))?;
        source.connect().await.map_err(|e| e.to_string())?;
    }
    save_current_config(&state)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn disconnect_source(
    state: tauri::State<'_, State>,
    source_name: String,
) -> Result<(), String> {
    {
        let mut s = state.write().await;
        let source = s
            .sources
            .get_mut(&source_name)
            .ok_or_else(|| format!("Unknown source: {}", source_name))?;
        source.disconnect().await.map_err(|e| e.to_string())?;
    }
    save_current_config(&state)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn sync_source(
    state: tauri::State<'_, State>,
    source_name: String,
) -> Result<(), String> {
    let state_inner = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::indexer::sync_source(&source_name, &state_inner).await {
            log::error!("Sync failed for {}: {}", source_name, e);
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn list_sources(state: tauri::State<'_, State>) -> Result<Vec<SourceStatus>, String> {
    let state = state.read().await;
    Ok(state.list_sources().await)
}

// ── Config persistence helper ─────────────────────────────────────────

async fn save_current_config(
    state: &tauri::State<'_, State>,
) -> Result<(), crate::error::AppError> {
    let app_state = state.read().await;
    let existing = config::load_config();
    let cfg = config::Config {
        clipboard_enabled: app_state.clipboard_enabled,
        skip_apps: existing.skip_apps,
        skip_title_patterns: existing.skip_title_patterns,
        private_browsing_detection: existing.private_browsing_detection,
        setup_completed: existing.setup_completed,
        anthropic_api_key: existing.anthropic_api_key,
        remote_access_enabled: existing.remote_access_enabled,
        screen_capture_enabled: app_state.screen_capture_enabled,
        ..existing
    };
    config::save_config(&cfg)
}

// ═══════════════════════════════════════════════════════════════════════
// DATA COMMANDS — proxied through the daemon via WenlanClient
// ═══════════════════════════════════════════════════════════════════════

// ── Search ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn search(
    state: tauri::State<'_, State>,
    query: String,
    limit: Option<usize>,
    source_filter: Option<String>,
) -> Result<Vec<SearchResult>, String> {
    let s = state.read().await;
    let req = requests::SearchRequest {
        query,
        limit: limit.unwrap_or(10),
        source_filter,
        space: None,
    };
    let resp: responses::SearchResponse = s.client.post_json("/api/search", &req).await?;
    Ok(resp.results)
}

#[tauri::command]
pub async fn search_memory(
    state: tauri::State<'_, State>,
    req: SearchMemoryRequest,
) -> Result<Vec<SearchResult>, String> {
    let s = state.read().await;
    let daemon_req = requests::SearchMemoryRequest {
        query: req.query,
        limit: req.limit.unwrap_or(10),
        memory_type: req.memory_type,
        space: req.domain,
        source_agent: req.source_agent,
        rerank: false,
    };
    let resp: responses::SearchMemoryResponse = s
        .client
        .post_json("/api/memory/search", &daemon_req)
        .await?;
    Ok(resp.results)
}

// ── Memory CRUD ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn store_memory(
    state: tauri::State<'_, State>,
    req: StoreMemoryRequest,
) -> Result<StoreMemoryResponse, String> {
    let s = state.read().await;
    let daemon_req = requests::StoreMemoryRequest {
        content: req.content,
        memory_type: req.memory_type,
        space: req.domain,
        source_agent: req.source_agent,
        title: req.title,
        confidence: req.confidence,
        supersedes: req.supersedes,
        entity: None,
        entity_id: None,
        structured_fields: req.structured_fields,
        retrieval_cue: req.retrieval_cue,
    };
    let resp: responses::StoreMemoryResponse =
        s.client.post_json("/api/memory/store", &daemon_req).await?;
    Ok(StoreMemoryResponse {
        source_id: resp.source_id,
        warnings: resp.warnings,
        enrichment: resp.enrichment,
        hint: resp.hint,
        triggered_revisions: resp.triggered_revisions,
        auto_superseded: resp.auto_superseded,
    })
}

#[tauri::command]
pub async fn confirm_memory(
    state: tauri::State<'_, State>,
    source_id: String,
    confirmed: bool,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::ConfirmRequest { confirmed };
    let _resp: responses::ConfirmResponse = s
        .client
        .post_json(&format!("/api/memory/confirm/{}", source_id), &req)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn set_stability_cmd(
    state: tauri::State<'_, State>,
    source_id: String,
    stability: String,
) -> Result<(), String> {
    // Stability is set via confirm for "confirmed" stability, otherwise via reclassify
    // For now, proxy confirm for "confirmed" and return error for others
    let s = state.read().await;
    if stability == "confirmed" {
        let req = requests::ConfirmRequest { confirmed: true };
        let _resp: responses::ConfirmResponse = s
            .client
            .post_json(&format!("/api/memory/confirm/{}", source_id), &req)
            .await?;
    } else {
        let req = requests::SetStabilityRequest { stability };
        let _resp: responses::SuccessResponse = s
            .client
            .put_json(&format!("/api/memory/{}/stability", source_id), &req)
            .await?;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_memories(
    state: tauri::State<'_, State>,
    req: ListMemoriesRequest,
) -> Result<Vec<IndexedFileInfo>, String> {
    let s = state.read().await;
    let daemon_req = requests::ListMemoriesRequest {
        memory_type: req.memory_type,
        space: req.domain,
        limit: req.limit.unwrap_or(100),
        confirmed: None,
    };
    let resp: responses::ListMemoriesResponse =
        s.client.post_json("/api/memory/list", &daemon_req).await?;
    Ok(resp.memories)
}

#[tauri::command]
pub async fn delete_memory(
    state: tauri::State<'_, State>,
    source_id: String,
) -> Result<(), String> {
    let s = state.read().await;
    let _resp: responses::DeleteResponse = s
        .client
        .delete_path(&format!("/api/memory/delete/{}", source_id))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn reclassify_memory_cmd(
    state: tauri::State<'_, State>,
    source_id: String,
    memory_type: String,
) -> Result<String, String> {
    let s = state.read().await;
    let req = requests::ReclassifyMemoryRequest { memory_type };
    let resp: responses::ReclassifyMemoryResponse = s
        .client
        .post_json(&format!("/api/memory/reclassify/{}", source_id), &req)
        .await?;
    Ok(resp.memory_type)
}

// ── Memory detail / list ──────────────────────────────────────────────

#[tauri::command]
pub async fn list_memories_cmd(
    state: tauri::State<'_, State>,
    domain: Option<String>,
    memory_type: Option<String>,
    confirmed: Option<bool>,
    limit: Option<usize>,
) -> Result<Vec<MemoryItem>, String> {
    let s = state.read().await;
    let daemon_req = requests::ListMemoriesRequest {
        memory_type,
        space: domain,
        limit: limit.unwrap_or(200),
        confirmed,
    };
    let resp: responses::ListMemoriesResponse =
        s.client.post_json("/api/memory/list", &daemon_req).await?;

    // The daemon returns IndexedFileInfo; the UI expects MemoryItem. Most
    // fields overlap; extras like entity_id and quality aren't surfaced by
    // the list endpoint and aren't needed for the list view.
    // Keep the client-side filter as a defensive fallback for older daemons.
    let items: Vec<MemoryItem> = resp
        .memories
        .into_iter()
        .filter(|info| match confirmed {
            Some(want) => info.confirmed == Some(want),
            None => true,
        })
        .map(|info| MemoryItem {
            source_id: info.source_id.clone(),
            title: info.title,
            content: info.content,
            summary: info.summary,
            memory_type: info.memory_type,
            space: info.space,
            source_agent: info.source_agent,
            confidence: info.confidence,
            confirmed: info.confirmed.unwrap_or(false),
            stability: info.stability,
            pinned: info.pinned,
            supersedes: None,
            last_modified: info.last_modified,
            chunk_count: info.chunk_count,
            entity_id: None,
            quality: None,
            is_recap: info.source_id.starts_with("recap_"),
            enrichment_status: String::from("raw"),
            supersede_mode: String::from("hide"),
            structured_fields: None,
            retrieval_cue: None,
            source_text: None,
            access_count: 0,
            version: 1,
            changelog: None,
            pending_revision: false,
            merged_from: None,
        })
        .collect();
    Ok(items)
}

#[tauri::command]
pub async fn get_memory_detail(
    state: tauri::State<'_, State>,
    source_id: String,
) -> Result<Option<MemoryItem>, String> {
    let client = state.read().await.client.clone();
    let resp: responses::MemoryDetailResponse = client
        .get_json(&format!("/api/memory/{}/detail", source_id))
        .await?;
    Ok(resp.memory)
}

#[tauri::command]
pub async fn get_enrichment_status(
    state: tauri::State<'_, State>,
    source_id: String,
) -> Result<wenlan_types::EnrichmentStatusResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.get_enrichment_status(&source_id).await
}

#[tauri::command]
pub async fn get_memory_revisions(
    state: tauri::State<'_, State>,
    source_id: String,
) -> Result<responses::ListMemoryRevisionsResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.get_memory_revisions(&source_id).await
}

#[tauri::command]
pub async fn list_memories_by_ids(
    state: tauri::State<'_, State>,
    ids: Vec<String>,
) -> Result<Vec<MemoryItem>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let client = state.read().await.client.clone();
    let ids_param = ids.join(",");
    let resp: responses::PinnedMemoriesResponse = client
        .get_json(&format!("/api/memory/by-ids?ids={}", ids_param))
        .await?;
    Ok(resp.memories)
}

#[tauri::command]
pub async fn get_memory_stats_cmd(state: tauri::State<'_, State>) -> Result<MemoryStats, String> {
    let s = state.read().await;
    let resp: responses::MemoryStatsResponse = s.client.get_json("/api/memory/stats").await?;
    Ok(resp.stats)
}

#[tauri::command]
pub async fn get_home_stats(state: tauri::State<'_, State>) -> Result<HomeStats, String> {
    let s = state.read().await;
    s.client.get_json::<HomeStats>("/api/home-stats").await
}

#[tauri::command]
pub async fn update_memory_cmd(
    state: tauri::State<'_, State>,
    source_id: String,
    content: Option<String>,
    domain: Option<String>,
    confirmed: Option<bool>,
    memory_type: Option<String>,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::UpdateMemoryRequest {
        content,
        space: domain,
        confirmed,
        memory_type,
    };
    let _resp: responses::SuccessResponse = s
        .client
        .put_json(&format!("/api/memory/{}/update", source_id), &req)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn get_version_chain_cmd(
    state: tauri::State<'_, State>,
    source_id: String,
) -> Result<Vec<MemoryVersionItem>, String> {
    let s = state.read().await;
    let resp: responses::VersionChainResponse = s
        .client
        .get_json(&format!("/api/memory/{}/versions", source_id))
        .await?;
    Ok(resp.versions)
}

// ── Indexed files / chunks ────────────────────────────────────────────

#[tauri::command]
pub async fn list_indexed_files(
    state: tauri::State<'_, State>,
) -> Result<Vec<IndexedFileInfo>, String> {
    let s = state.read().await;
    let resp: responses::IndexedFilesResponse = s.client.get_json("/api/indexed-files").await?;
    Ok(resp.files)
}

#[tauri::command]
pub async fn get_chunks(
    state: tauri::State<'_, State>,
    _source: String,
    source_id: String,
) -> Result<Vec<MemoryDetail>, String> {
    let s = state.read().await;
    let chunks: Vec<MemoryDetail> = s
        .client
        .get_json(&format!("/api/chunks/{}", source_id))
        .await?;
    Ok(chunks)
}

#[tauri::command]
pub async fn update_chunk(
    state: tauri::State<'_, State>,
    id: String,
    content: String,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::UpdateChunkRequest { content };
    let _resp: responses::SuccessResponse = s
        .client
        .put_json(&format!("/api/chunks/{}/update", id), &req)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_file_chunks(
    state: tauri::State<'_, State>,
    source: String,
    source_id: String,
) -> Result<(), String> {
    let s = state.read().await;
    let _resp: responses::DeleteResponse = s
        .client
        .delete_path(&format!("/api/documents/{}/{}", source, source_id))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_by_time_range(
    state: tauri::State<'_, State>,
    start: i64,
    end: i64,
) -> Result<(), String> {
    // Remove from in-memory activity list
    {
        let mut s = state.write().await;
        s.completed_activities
            .retain(|a| !(a.started_at <= end && a.ended_at >= start));
        s.save_all_activities();
    }
    let s = state.read().await;
    let req = requests::DeleteByTimeRangeRequest { start, end };
    let _resp: responses::DeleteCountResponse =
        s.client.delete_json("/api/chunks/time-range", &req).await?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct BulkDeleteItem {
    pub source: String,
    pub source_id: String,
}

#[tauri::command]
pub async fn delete_bulk(
    state: tauri::State<'_, State>,
    items: Vec<BulkDeleteItem>,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::BulkDeleteRequest {
        items: items
            .into_iter()
            .map(|i| requests::BulkDeleteItem {
                source: i.source,
                source_id: i.source_id,
            })
            .collect(),
    };
    let _resp: responses::DeleteCountResponse =
        s.client.post_json("/api/chunks/delete-bulk", &req).await?;
    Ok(())
}

// ── Quick capture / ingest ────────────────────────────────────────────

#[tauri::command]
pub async fn quick_capture(
    state: tauri::State<'_, State>,
    req: QuickCaptureRequest,
) -> Result<usize, String> {
    let source_id = format!("manual_{}", chrono::Utc::now().timestamp());

    let title = req.title.unwrap_or_else(|| {
        let first_line = req.content.lines().next().unwrap_or("Untitled");
        if first_line.chars().count() > 60 {
            format!("{}...", first_line.chars().take(60).collect::<String>())
        } else {
            first_line.to_string()
        }
    });

    let mut metadata = HashMap::new();
    if let Some(tags) = &req.tags {
        metadata.insert("tags".to_string(), tags.join(","));
    }
    // IngestMemoryRequest doesn't have typed memory_type/domain fields, so
    // forward them via metadata. The daemon's post-ingest enrichment can
    // pick them up as hints; otherwise they stay as searchable metadata.
    if let Some(ref mt) = req.memory_type {
        metadata.insert("memory_type".to_string(), mt.clone());
    }
    if let Some(ref d) = req.domain {
        metadata.insert("domain".to_string(), d.clone());
    }

    let s = state.read().await;
    let ingest_req = requests::IngestMemoryRequest {
        source: "manual".to_string(),
        source_id: source_id.clone(),
        title,
        content: req.content,
        url: None,
        tags: req.tags,
        metadata: Some(metadata),
    };
    let resp: responses::IngestResponse = s
        .client
        .post_json("/api/ingest/memory", &ingest_req)
        .await?;
    Ok(resp.chunks_created)
}

#[tauri::command]
pub async fn ingest_clipboard(
    state: tauri::State<'_, State>,
    content: String,
) -> Result<usize, String> {
    let trimmed = content.trim();
    if trimmed.len() < 4 {
        return Ok(0);
    }

    // Generate deterministic source_id from content hash
    let hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    };
    let source_id = format!("clipboard_{}", &hash[..12.min(hash.len())]);

    let title = {
        let first_line = trimmed.lines().next().unwrap_or("Clipboard");
        if first_line.chars().count() > 60 {
            format!("{}...", first_line.chars().take(60).collect::<String>())
        } else {
            first_line.to_string()
        }
    };

    let s = state.read().await;
    let ingest_req = requests::IngestTextRequest {
        source: "clipboard".to_string(),
        source_id,
        title,
        content,
        url: None,
        metadata: None,
    };
    let resp: responses::IngestResponse =
        s.client.post_json("/api/ingest/text", &ingest_req).await?;
    Ok(resp.chunks_created)
}

#[tauri::command]
pub async fn import_memories_cmd(
    state: tauri::State<'_, State>,
    app_handle: tauri::AppHandle,
    source: String,
    content: String,
    _label: Option<String>,
) -> Result<responses::ImportMemoriesResponse, String> {
    let s = state.read().await;
    let req = requests::ImportMemoriesRequest {
        source,
        content,
        label: _label,
    };
    let result: responses::ImportMemoriesResponse =
        s.client.post_json("/api/import/memories", &req).await?;

    // Emit event for UI refresh
    use tauri::Emitter;
    let _ = app_handle.emit("import-complete", &result);

    Ok(result)
}

#[tauri::command]
pub async fn import_chat_export(
    state: tauri::State<'_, State>,
    path: String,
) -> Result<wenlan_types::import::ImportChatExportResponse, String> {
    let s = state.read().await;
    s.client.import_chat_export(&path).await
}

#[tauri::command]
pub async fn list_pending_imports(
    state: tauri::State<'_, State>,
) -> Result<Vec<wenlan_types::import::PendingImport>, String> {
    let s = state.read().await;
    s.client.list_pending_imports().await
}

// ── Onboarding milestones ───────────────────────────────────────────

#[tauri::command]
pub async fn list_onboarding_milestones(
    state: tauri::State<'_, State>,
) -> Result<Vec<wenlan_types::onboarding::MilestoneRecord>, String> {
    // Snapshot the client out of the guard so we never hold the RwLock across
    // the HTTP .await — holding it would block all writers for the duration
    // of the request. `WenlanClient` is `Clone` and cheap to clone.
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.list_onboarding_milestones().await
}

#[tauri::command]
pub async fn acknowledge_onboarding_milestone(
    state: tauri::State<'_, State>,
    id: String,
) -> Result<(), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.acknowledge_onboarding_milestone(&id).await
}

#[tauri::command]
pub async fn reset_onboarding_milestones(state: tauri::State<'_, State>) -> Result<(), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.reset_onboarding_milestones().await
}

#[tauri::command]
pub async fn save_temp_file(bytes: Vec<u8>, filename: String) -> Result<String, String> {
    let dir = std::env::temp_dir().join("origin-chat-import");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir temp: {e}"))?;
    let safe: String = filename
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
        .collect();
    // Prevent path traversal: ".." passes the char filter since "." is allowed.
    // Also reject empty filenames (e.g., input was all slashes).
    if safe.is_empty() || safe == "." || safe == ".." {
        return Err("Invalid filename".to_string());
    }
    // Add UUID prefix to prevent overwrites between concurrent imports.
    let unique_name = format!("{}_{}", uuid::Uuid::new_v4(), safe);
    let path = dir.join(&unique_name);
    std::fs::write(&path, &bytes).map_err(|e| format!("write temp: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

// ── Knowledge graph / entities ────────────────────────────────────────

#[tauri::command]
pub async fn create_entity_cmd(
    state: tauri::State<'_, State>,
    name: String,
    entity_type: String,
    domain: Option<String>,
) -> Result<String, String> {
    let s = state.read().await;
    let req = requests::CreateEntityRequest {
        name,
        entity_type,
        space: domain,
        source_agent: None,
        confidence: None,
    };
    let resp: responses::CreateEntityResponse =
        s.client.post_json("/api/memory/entities", &req).await?;
    Ok(resp.id)
}

#[tauri::command]
pub async fn list_entities_cmd(
    state: tauri::State<'_, State>,
    entity_type: Option<String>,
    domain: Option<String>,
) -> Result<Vec<Entity>, String> {
    let s = state.read().await;
    let req = requests::ListEntitiesRequest {
        entity_type,
        space: domain,
    };
    let resp: responses::ListEntitiesResponse = s
        .client
        .post_json("/api/memory/entities/list", &req)
        .await?;
    Ok(resp.entities)
}

#[tauri::command]
pub async fn search_entities_cmd(
    state: tauri::State<'_, State>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<EntitySearchResult>, String> {
    let s = state.read().await;
    let req = requests::SearchEntitiesRequest {
        query,
        limit: limit.unwrap_or(20),
    };
    let resp: responses::SearchEntitiesResponse = s
        .client
        .post_json("/api/memory/entities/search", &req)
        .await?;
    Ok(resp.results)
}

#[tauri::command]
pub async fn get_entity_detail_cmd(
    state: tauri::State<'_, State>,
    entity_id: String,
) -> Result<EntityDetail, String> {
    let s = state.read().await;
    s.client
        .get_json(&format!("/api/memory/entities/{}", entity_id))
        .await
}

#[tauri::command]
pub async fn update_observation_cmd(
    state: tauri::State<'_, State>,
    observation_id: String,
    content: String,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::UpdateObservationRequest { content };
    let _resp: responses::SuccessResponse = s
        .client
        .put_json(
            &format!("/api/memory/observations/{}", observation_id),
            &req,
        )
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_observation_cmd(
    state: tauri::State<'_, State>,
    observation_id: String,
) -> Result<(), String> {
    let s = state.read().await;
    let _resp: responses::SuccessResponse = s
        .client
        .delete_path(&format!("/api/memory/observations/{}", observation_id))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_entity_cmd(
    state: tauri::State<'_, State>,
    entity_id: String,
) -> Result<(), String> {
    let s = state.read().await;
    let _resp: responses::SuccessResponse = s
        .client
        .delete_path(&format!("/api/memory/entities/{}/delete", entity_id))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn confirm_entity_cmd(
    state: tauri::State<'_, State>,
    entity_id: String,
    confirmed: bool,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::ConfirmEntityRequest { confirmed };
    let _resp: responses::SuccessResponse = s
        .client
        .put_json(&format!("/api/memory/entities/{}/confirm", entity_id), &req)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn confirm_observation_cmd(
    state: tauri::State<'_, State>,
    observation_id: String,
    confirmed: bool,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::ConfirmObservationRequest { confirmed };
    let _resp: responses::SuccessResponse = s
        .client
        .put_json(
            &format!("/api/memory/observations/{}/confirm", observation_id),
            &req,
        )
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn add_observation_cmd(
    state: tauri::State<'_, State>,
    entity_id: String,
    content: String,
    source_agent: Option<String>,
    confidence: Option<f32>,
) -> Result<String, String> {
    let s = state.read().await;
    let req = requests::AddObservationRequest {
        entity_id,
        content,
        source_agent,
        confidence,
    };
    let resp: responses::AddObservationResponse =
        s.client.post_json("/api/memory/observations", &req).await?;
    Ok(resp.id)
}

// ── Profile & agents ──────────────────────────────────────────────────

#[tauri::command]
pub async fn get_profile(state: tauri::State<'_, State>) -> Result<Option<Profile>, String> {
    let s = state.read().await;
    match s
        .client
        .get_json::<responses::ProfileResponse>("/api/profile")
        .await
    {
        Ok(resp) => Ok(Some(Profile {
            id: resp.id,
            name: resp.name,
            display_name: resp.display_name,
            email: resp.email,
            bio: resp.bio,
            avatar_path: resolve_profile_avatar_path(resp.avatar_path),
            created_at: resp.created_at,
            updated_at: resp.updated_at,
        })),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn update_profile(
    state: tauri::State<'_, State>,
    _id: String,
    name: Option<String>,
    display_name: Option<String>,
    email: Option<String>,
    bio: Option<String>,
    avatar_path: Option<String>,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::UpdateProfileRequest {
        name,
        display_name,
        email,
        bio,
        avatar_path,
    };
    let _resp: responses::ProfileResponse = s.client.put_json("/api/profile", &req).await?;
    Ok(())
}

#[tauri::command]
pub async fn list_agents(state: tauri::State<'_, State>) -> Result<Vec<AgentConnection>, String> {
    let s = state.read().await;
    let agents: Vec<responses::AgentResponse> = s.client.get_json("/api/agents").await?;
    Ok(agents
        .into_iter()
        .map(|a| AgentConnection {
            id: a.id,
            name: a.name,
            display_name: a.display_name,
            agent_type: a.agent_type,
            description: a.description,
            enabled: a.enabled,
            trust_level: a.trust_level,
            last_seen_at: a.last_seen_at,
            memory_count: a.memory_count,
            created_at: a.created_at,
            updated_at: a.updated_at,
        })
        .collect())
}

#[tauri::command]
pub async fn get_agent(
    state: tauri::State<'_, State>,
    name: String,
) -> Result<Option<AgentConnection>, String> {
    let s = state.read().await;
    match s
        .client
        .get_json::<responses::AgentResponse>(&format!("/api/agents/{}", name))
        .await
    {
        Ok(a) => Ok(Some(AgentConnection {
            id: a.id,
            name: a.name,
            display_name: a.display_name,
            agent_type: a.agent_type,
            description: a.description,
            enabled: a.enabled,
            trust_level: a.trust_level,
            last_seen_at: a.last_seen_at,
            memory_count: a.memory_count,
            created_at: a.created_at,
            updated_at: a.updated_at,
        })),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn update_agent(
    state: tauri::State<'_, State>,
    name: String,
    agent_type: Option<String>,
    description: Option<String>,
    enabled: Option<bool>,
    trust_level: Option<String>,
    display_name: Option<String>,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::UpdateAgentRequest {
        agent_type,
        description,
        enabled,
        trust_level,
        display_name,
    };
    let _resp: responses::AgentResponse = s
        .client
        .put_json(&format!("/api/agents/{}", name), &req)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_agent(state: tauri::State<'_, State>, name: String) -> Result<(), String> {
    let s = state.read().await;
    let _resp: serde_json::Value = s
        .client
        .delete_path(&format!("/api/agents/{}", name))
        .await?;
    Ok(())
}

// ── Avatar commands ───────────────────────────────────────────────────

fn avatar_storage_dir() -> PathBuf {
    crate::identity_paths::app_data_dir().join("avatars")
}

fn legacy_avatar_storage_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(custom) = std::env::var_os("ORIGIN_DATA_DIR") {
        dirs.push(PathBuf::from(custom).join("avatars"));
    }
    dirs.push(crate::identity_paths::legacy_app_data_dir().join("avatars"));
    dirs
}

fn resolve_profile_avatar_path(avatar_path: Option<String>) -> Option<String> {
    let avatar_path = avatar_path?;
    if avatar_path.is_empty() {
        return None;
    }

    let path = PathBuf::from(&avatar_path);
    if path.exists() {
        return Some(avatar_path);
    }

    let parent = path.parent()?;
    if !legacy_avatar_storage_dirs()
        .iter()
        .any(|legacy_dir| legacy_dir == parent)
    {
        return None;
    }

    let filename = path.file_name()?;
    let migrated = avatar_storage_dir().join(filename);
    if migrated.exists() {
        return Some(migrated.to_string_lossy().to_string());
    }

    None
}

#[tauri::command]
pub async fn set_avatar(
    state: tauri::State<'_, State>,
    source_path: String,
) -> Result<String, String> {
    let source = std::path::Path::new(&source_path);
    if !source.exists() {
        return Err(format!("Source file not found: {}", source_path));
    }

    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("png");

    let avatars_dir = avatar_storage_dir();
    std::fs::create_dir_all(&avatars_dir)
        .map_err(|e| format!("Failed to create avatars directory: {}", e))?;

    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let dest = avatars_dir.join(&filename);

    std::fs::copy(source, &dest).map_err(|e| format!("Failed to copy avatar file: {}", e))?;

    let dest_str = dest.to_string_lossy().to_string();

    // Update profile via daemon
    let s = state.read().await;
    let req = requests::UpdateProfileRequest {
        name: None,
        display_name: None,
        email: None,
        bio: None,
        avatar_path: Some(dest_str.clone()),
    };
    let _resp: responses::ProfileResponse = s.client.put_json("/api/profile", &req).await?;

    Ok(dest_str)
}

#[tauri::command]
pub async fn get_avatar_data_url(state: tauri::State<'_, State>) -> Result<Option<String>, String> {
    let s = state.read().await;
    let profile = match s
        .client
        .get_json::<responses::ProfileResponse>("/api/profile")
        .await
    {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };
    let Some(avatar_path) = resolve_profile_avatar_path(profile.avatar_path) else {
        return Ok(None);
    };

    let path = std::path::Path::new(&avatar_path);
    if !path.exists() {
        return Ok(None);
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
    let mime = match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/png",
    };

    let bytes = std::fs::read(path).map_err(|e| format!("Failed to read avatar: {}", e))?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(Some(format!("data:{};base64,{}", mime, b64)))
}

#[tauri::command]
pub async fn remove_avatar(state: tauri::State<'_, State>) -> Result<(), String> {
    let s = state.read().await;
    let profile = match s
        .client
        .get_json::<responses::ProfileResponse>("/api/profile")
        .await
    {
        Ok(p) => p,
        Err(_) => return Ok(()),
    };

    if let Some(avatar_path) = resolve_profile_avatar_path(profile.avatar_path) {
        let _ = std::fs::remove_file(avatar_path);
    }

    let req = requests::UpdateProfileRequest {
        name: None,
        display_name: None,
        email: None,
        bio: None,
        avatar_path: Some(String::new()),
    };
    let _resp: responses::ProfileResponse = s.client.put_json("/api/profile", &req).await?;
    Ok(())
}

// ── Pin/unpin ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn pin_memory(state: tauri::State<'_, State>, source_id: String) -> Result<(), String> {
    let s = state.read().await;
    let _resp: responses::SuccessResponse = s
        .client
        .post_empty(&format!("/api/memory/{}/pin", source_id))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn unpin_memory(state: tauri::State<'_, State>, source_id: String) -> Result<(), String> {
    let s = state.read().await;
    let _resp: responses::SuccessResponse = s
        .client
        .post_empty(&format!("/api/memory/{}/unpin", source_id))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn list_pinned_memories(
    state: tauri::State<'_, State>,
) -> Result<Vec<MemoryItem>, String> {
    let s = state.read().await;
    let resp: responses::PinnedMemoriesResponse = s.client.get_json("/api/memory/pinned").await?;
    Ok(resp.memories)
}

// ── Pending revisions ─────────────────────────────────────────────────

#[tauri::command]
pub async fn accept_pending_revision(
    state: tauri::State<'_, State>,
    source_id: String,
) -> Result<responses::RevisionAcceptResponse, String> {
    let s = state.read().await;
    s.client
        .post_empty(&format!("/api/memory/revision/{}/accept", source_id))
        .await
}

#[tauri::command]
pub async fn dismiss_pending_revision(
    state: tauri::State<'_, State>,
    source_id: String,
) -> Result<responses::RevisionDismissResponse, String> {
    let s = state.read().await;
    s.client
        .post_empty(&format!("/api/memory/revision/{}/dismiss", source_id))
        .await
}

// ── Contradiction flags ────────────────────────────────────────────────

#[tauri::command]
pub async fn dismiss_contradiction(
    state: tauri::State<'_, State>,
    source_id: String,
) -> Result<responses::ContradictionDismissResponse, String> {
    let s = state.read().await;
    s.client
        .post_empty(&format!("/api/memory/contradiction/{}/dismiss", source_id))
        .await
}

// ── Refinery queue ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_refinements(
    state: tauri::State<'_, State>,
    limit: Option<usize>,
) -> Result<responses::ListRefinementsResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.list_refinements(limit).await
}

#[tauri::command]
pub async fn accept_refinement(
    state: tauri::State<'_, State>,
    id: String,
) -> Result<responses::AcceptRefinementResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.accept_refinement(&id).await
}

#[tauri::command]
pub async fn reject_refinement(
    state: tauri::State<'_, State>,
    id: String,
) -> Result<responses::RejectRefinementResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.reject_refinement(&id).await
}

#[tauri::command]
pub async fn get_pending_revision(
    state: tauri::State<'_, State>,
    source_id: String,
) -> Result<Option<PendingRevision>, String> {
    let s = state.read().await;
    let revision: Option<PendingRevision> = s
        .client
        .get_json(&format!("/api/memory/pending-revision/{}", source_id))
        .await?;
    Ok(revision)
}

#[tauri::command]
pub async fn list_pending_revisions(
    state: tauri::State<'_, State>,
    limit: Option<usize>,
) -> Result<Vec<responses::PendingRevisionItem>, String> {
    let s = state.read().await;
    let path = match limit {
        Some(limit) => format!("/api/memory/pending-revisions?limit={limit}"),
        None => "/api/memory/pending-revisions".to_string(),
    };
    s.client.get_json(&path).await
}

// ── Briefing / narrative ──────────────────────────────────────────────

#[tauri::command]
pub async fn get_briefing(state: tauri::State<'_, State>) -> Result<BriefingResponse, String> {
    let s = state.read().await;
    let resp: BriefingResponse = s.client.get_json("/api/briefing").await?;
    Ok(resp)
}

#[tauri::command]
pub async fn get_pending_contradictions(
    _state: tauri::State<'_, State>,
) -> Result<Vec<ContradictionItem>, String> {
    Ok(vec![])
}

#[tauri::command]
pub async fn get_profile_narrative(
    state: tauri::State<'_, State>,
) -> Result<NarrativeResponse, String> {
    let s = state.read().await;
    let resp: NarrativeResponse = s.client.get_json("/api/profile/narrative").await?;
    Ok(resp)
}

#[tauri::command]
pub async fn regenerate_narrative(
    state: tauri::State<'_, State>,
) -> Result<NarrativeResponse, String> {
    let s = state.read().await;
    let resp: NarrativeResponse = s
        .client
        .post_empty("/api/profile/narrative/regenerate")
        .await?;
    Ok(resp)
}

// ── Agent activity ────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_agent_activity(
    state: tauri::State<'_, State>,
    limit: Option<usize>,
    agent_name: Option<String>,
    since: Option<i64>,
) -> Result<Vec<AgentActivityRow>, String> {
    let s = state.read().await;
    let mut path = format!("/api/activities?limit={}", limit.unwrap_or(50));
    if let Some(name) = agent_name {
        path.push_str(&format!("&agent_name={}", name));
    }
    if let Some(since_val) = since {
        path.push_str(&format!("&since={}", since_val));
    }
    let resp: responses::ActivityResponse = s.client.get_json(&path).await?;
    Ok(resp.activities)
}

// ── Entity suggestions ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EntitySuggestion {
    pub id: String,
    pub entity_name: Option<String>,
    pub source_ids: Vec<String>,
    pub confidence: f64,
    pub created_at: String,
}

#[tauri::command]
pub async fn get_entity_suggestions_cmd(
    state: tauri::State<'_, State>,
) -> Result<Vec<EntitySuggestion>, String> {
    let s = state.read().await;
    let suggestions: Vec<wenlan_types::EntitySuggestion> =
        s.client.get_json("/api/memory/entity-suggestions").await?;
    Ok(suggestions
        .into_iter()
        .map(|s| EntitySuggestion {
            id: s.id,
            entity_name: s.entity_name,
            source_ids: s.source_ids,
            confidence: s.confidence,
            created_at: s.created_at,
        })
        .collect())
}

#[tauri::command]
pub async fn approve_entity_suggestion_cmd(
    state: tauri::State<'_, State>,
    id: String,
) -> Result<responses::AcceptRefinementResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.accept_refinement(&id).await
}

#[tauri::command]
pub async fn dismiss_entity_suggestion_cmd(
    state: tauri::State<'_, State>,
    id: String,
) -> Result<responses::RejectRefinementResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.reject_refinement(&id).await
}

#[cfg(test)]
mod entity_suggestion_command_type_tests {
    use super::*;

    #[allow(dead_code)]
    async fn approve_entity_suggestion_uses_refinery_accept_response(
        state: tauri::State<'_, State>,
    ) {
        let _: Result<responses::AcceptRefinementResponse, String> =
            approve_entity_suggestion_cmd(state, String::new()).await;
    }

    #[allow(dead_code)]
    async fn dismiss_entity_suggestion_uses_refinery_reject_response(
        state: tauri::State<'_, State>,
    ) {
        let _: Result<responses::RejectRefinementResponse, String> =
            dismiss_entity_suggestion_cmd(state, String::new()).await;
    }

    #[test]
    fn entity_suggestion_command_response_types_are_checked() {}
}

// ── Spaces ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_spaces(state: tauri::State<'_, State>) -> Result<Vec<Space>, String> {
    let s = state.read().await;
    s.client.get_json("/api/spaces").await
}

#[tauri::command]
pub async fn get_space(
    state: tauri::State<'_, State>,
    name: String,
) -> Result<Option<Space>, String> {
    // No direct get-by-name endpoint, but we can list and filter
    let s = state.read().await;
    let spaces: Vec<Space> = s.client.get_json("/api/spaces").await?;
    Ok(spaces.into_iter().find(|sp| sp.name == name))
}

#[derive(Debug, Deserialize)]
struct DeleteSpaceResponse {
    deleted: String,
}

#[derive(Debug, Deserialize)]
struct ToggleSpaceStarredResponse {
    starred: bool,
}

async fn delete_space_response(
    client: &crate::api::WenlanClient,
    name: &str,
) -> Result<DeleteSpaceResponse, String> {
    client.delete_path(&format!("/api/spaces/{}", name)).await
}

async fn toggle_space_starred_response(
    client: &crate::api::WenlanClient,
    name: &str,
) -> Result<ToggleSpaceStarredResponse, String> {
    client
        .post_empty(&format!("/api/spaces/{}/star", name))
        .await
}

#[tauri::command]
pub async fn create_space(
    state: tauri::State<'_, State>,
    name: String,
    description: Option<String>,
) -> Result<Space, String> {
    let s = state.read().await;
    let req = requests::CreateSpaceRequest { name, description };
    s.client.post_json("/api/spaces", &req).await
}

#[tauri::command]
pub async fn update_space(
    state: tauri::State<'_, State>,
    name: String,
    new_name: String,
    description: Option<String>,
) -> Result<Space, String> {
    let s = state.read().await;
    let req = requests::UpdateSpaceRequest {
        new_name: Some(new_name),
        description,
    };
    s.client
        .put_json(&format!("/api/spaces/{}", name), &req)
        .await
}

#[tauri::command]
pub async fn delete_space(
    state: tauri::State<'_, State>,
    name: String,
    _memory_action: Option<String>,
) -> Result<(), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    let DeleteSpaceResponse { deleted: _deleted } = delete_space_response(&client, &name).await?;
    Ok(())
}

#[tauri::command]
pub async fn confirm_space(state: tauri::State<'_, State>, name: String) -> Result<(), String> {
    let s = state.read().await;
    let _resp: responses::SuccessResponse = s
        .client
        .post_empty(&format!("/api/spaces/{}/confirm", name))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn reorder_space(
    state: tauri::State<'_, State>,
    name: String,
    new_order: i64,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::ReorderSpaceRequest { name, new_order };
    let _resp: responses::SuccessResponse = s.client.post_json("/api/spaces/reorder", &req).await?;
    Ok(())
}

#[tauri::command]
pub async fn toggle_space_starred(
    state: tauri::State<'_, State>,
    name: String,
) -> Result<bool, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    let resp = toggle_space_starred_response(&client, &name).await?;
    Ok(resp.starred)
}

// Legacy space commands (local SpaceStore — these are being superseded by daemon spaces)
#[tauri::command]
pub async fn set_document_space(
    state: tauri::State<'_, State>,
    _source: String,
    source_id: String,
    space_id: String,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::SetDocumentSpaceRequest {
        space_name: space_id,
    };
    let _resp: responses::SuccessResponse = s
        .client
        .post_json(&format!("/api/documents/{}/space", source_id), &req)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn add_space(
    state: tauri::State<'_, State>,
    name: String,
    _icon: String,
    _color: String,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::CreateSpaceRequest {
        name,
        description: None,
    };
    let _space: Space = s.client.post_json("/api/spaces", &req).await?;
    Ok(())
}

#[tauri::command]
pub async fn remove_space(state: tauri::State<'_, State>, space_id: String) -> Result<(), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    let DeleteSpaceResponse { deleted: _deleted } =
        delete_space_response(&client, &space_id).await?;
    Ok(())
}

#[tauri::command]
pub async fn rename_space(
    state: tauri::State<'_, State>,
    space_id: String,
    new_name: String,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::UpdateSpaceRequest {
        new_name: Some(new_name),
        description: None,
    };
    let _space: Space = s
        .client
        .put_json(&format!("/api/spaces/{}", space_id), &req)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn pin_space(state: tauri::State<'_, State>, space_id: String) -> Result<(), String> {
    let s = state.read().await;
    let _resp: responses::SuccessResponse = s
        .client
        .post_empty(&format!("/api/spaces/{}/pin", space_id))
        .await?;
    Ok(())
}

#[cfg(test)]
mod space_command_type_tests {
    use super::*;

    #[allow(dead_code)]
    async fn delete_space_response_uses_typed_deleted_envelope(client: crate::api::WenlanClient) {
        let _: Result<DeleteSpaceResponse, String> = delete_space_response(&client, "space").await;
    }

    #[allow(dead_code)]
    async fn toggle_space_starred_response_uses_typed_starred_envelope(
        client: crate::api::WenlanClient,
    ) {
        let _: Result<ToggleSpaceStarredResponse, String> =
            toggle_space_starred_response(&client, "space").await;
    }

    #[allow(dead_code)]
    async fn legacy_space_aliases_keep_void_tauri_surface(state: tauri::State<'_, State>) {
        let _: Result<(), String> =
            add_space(state.clone(), String::new(), String::new(), String::new()).await;
        let _: Result<(), String> = remove_space(state.clone(), String::new()).await;
        let _: Result<(), String> = rename_space(state, String::new(), String::new()).await;
    }

    #[test]
    fn space_response_envelopes_deserialize_daemon_payloads() {
        let deleted: DeleteSpaceResponse = serde_json::from_value(serde_json::json!({
            "deleted": "Engineering"
        }))
        .unwrap();
        assert_eq!(deleted.deleted, "Engineering");

        let starred: ToggleSpaceStarredResponse = serde_json::from_value(serde_json::json!({
            "starred": true
        }))
        .unwrap();
        assert!(starred.starred);
    }
}

// ── Tags ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct TagData {
    pub tags: Vec<String>,
    pub document_tags: HashMap<String, Vec<String>>,
    pub categories: Vec<String>,
    pub document_categories: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct SetDocumentTagsRequest {
    source: String,
    tags: Vec<String>,
}

#[tauri::command]
pub async fn list_all_tags(state: tauri::State<'_, State>) -> Result<TagData, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    let inventory = client.list_tag_inventory().await?;
    Ok(tag_data_from_inventory(inventory))
}

fn tag_data_from_inventory(inventory: crate::api::TagInventoryResponse) -> TagData {
    TagData {
        tags: inventory.tags,
        document_tags: inventory.document_tags,
        categories: vec![],
        document_categories: HashMap::new(),
    }
}

#[tauri::command]
pub async fn set_document_tags(
    state: tauri::State<'_, State>,
    source: String,
    source_id: String,
    tags: Vec<String>,
) -> Result<Vec<String>, String> {
    let client = state.read().await.client.clone();
    let req = SetDocumentTagsRequest { source, tags };
    let resp: responses::TagsResponse = client
        .put_json(&format!("/api/documents/{}/tags", source_id), &req)
        .await?;
    Ok(resp.tags)
}

#[tauri::command]
pub async fn delete_tag(state: tauri::State<'_, State>, name: String) -> Result<(), String> {
    let client = state.read().await.client.clone();
    let _resp: responses::SuccessResponse =
        client.delete_path(&format!("/api/tags/{}", name)).await?;
    Ok(())
}

#[tauri::command]
pub async fn suggest_tags(
    state: tauri::State<'_, State>,
    source: String,
    source_id: String,
    last_modified: i64,
) -> Result<Vec<String>, String> {
    // Snapshot everything we need from AppState inside a scoped block so
    // the read guard is dropped before the HTTP call. Holding a
    // `tokio::sync::RwLock` read guard across `.await` would block any
    // writer (config updates, sensor toggles, etc.) for the full
    // duration of the round-trip. See CLAUDE.md "Async and locking".
    //
    // Local signal: the app that was active at the document's timestamp.
    // Activities are tracked in-process by the Tauri app (the daemon has
    // no view of them), so look the app name up here and pass it to the
    // daemon as a merge hint.
    let (client, activity_app): (crate::api::WenlanClient, Option<String>) = {
        let s = state.read().await;
        let activity_app = s
            .list_activity_summaries()
            .into_iter()
            .find(|a| last_modified >= a.started_at && last_modified <= a.ended_at)
            .and_then(|a| a.app_names.first().cloned());
        (s.client.clone(), activity_app)
    }; // guard dropped here

    // Build the query string with percent-encoded values. Using a
    // minimal encoder — source/source_id are usually simple ASCII
    // identifiers but may contain spaces or slashes, and the app name
    // commonly has spaces. Matches RFC 3986 unreserved set.
    let mut path = String::from("/api/suggest-tags?source=");
    path.push_str(&percent_encode(&source));
    path.push_str("&source_id=");
    path.push_str(&percent_encode(&source_id));
    if let Some(ref app) = activity_app {
        path.push_str("&activity_app=");
        path.push_str(&percent_encode(app));
    }

    let resp: responses::TagsResponse = client.get_json(&path).await?;
    Ok(resp.tags)
}

/// Percent-encode a string for inclusion in a URL query parameter value.
/// Encodes every byte that isn't in the RFC 3986 unreserved set
/// (alphanumeric + `-._~`).
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

// ── Working memory / sessions ─────────────────────────────────────────

#[tauri::command]
pub async fn get_working_memory(
    state: tauri::State<'_, State>,
) -> Result<Vec<wenlan_types::working_memory::WorkingMemoryEntry>, String> {
    // Working memory is in-process state populated by the context consumer
    // (`router/intent.rs`). Sensors run only in the app process, so this
    // is served from the app's local rolling buffer rather than an HTTP endpoint.
    let wm_handle = {
        let s = state.read().await;
        s.working_memory.clone()
    };
    let mut wm = wm_handle.lock().await;
    Ok(wm.get_recent())
}

#[tauri::command]
pub async fn get_session_snapshots(
    state: tauri::State<'_, State>,
    limit: Option<usize>,
) -> Result<Vec<wenlan_types::SessionSnapshot>, String> {
    let s = state.read().await;
    let path = format!("/api/snapshots?limit={}", limit.unwrap_or(10));
    s.client.get_json(&path).await
}

#[tauri::command]
pub async fn get_snapshot_captures(
    state: tauri::State<'_, State>,
    snapshot_id: String,
) -> Result<Vec<wenlan_types::SnapshotCapture>, String> {
    let s = state.read().await;
    s.client
        .get_json(&format!("/api/snapshots/{}/captures", snapshot_id))
        .await
}

#[tauri::command]
pub async fn get_snapshot_captures_with_content(
    state: tauri::State<'_, State>,
    snapshot_id: String,
) -> Result<Vec<wenlan_types::SnapshotCaptureWithContent>, String> {
    let s = state.read().await;
    s.client
        .get_json(&format!(
            "/api/snapshots/{}/captures-with-content",
            snapshot_id
        ))
        .await
}

#[tauri::command]
pub async fn delete_snapshot(
    state: tauri::State<'_, State>,
    snapshot_id: String,
) -> Result<(), String> {
    let s = state.read().await;
    let _resp: responses::SuccessResponse = s
        .client
        .post_empty(&format!("/api/snapshots/{}/delete", snapshot_id))
        .await?;
    Ok(())
}

// ── Memory nurture ────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_nurture_cards_cmd(
    state: tauri::State<'_, State>,
    _limit: Option<usize>,
    _domain: Option<String>,
) -> Result<Vec<MemoryItem>, String> {
    let s = state.read().await;
    let resp: responses::NurtureCardsResponse = s.client.get_json("/api/memory/nurture").await?;
    Ok(resp.cards)
}

#[tauri::command]
pub async fn correct_memory_cmd(
    state: tauri::State<'_, State>,
    source_id: String,
    correction_prompt: String,
) -> Result<String, String> {
    let s = state.read().await;
    let req = requests::CorrectMemoryRequest { correction_prompt };
    let resp: serde_json::Value = s
        .client
        .post_json(&format!("/api/memory/{}/correct", source_id), &req)
        .await?;
    Ok(resp
        .get("corrected")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

// ── Pages ──────────────────────────────────────────────────────────

/// Wire wrapper for the daemon's `{ "page": {...} }` response shape.
#[derive(serde::Deserialize)]
struct GetPageWire {
    page: Option<Page>,
}

#[tauri::command]
pub async fn get_page(state: tauri::State<'_, State>, id: String) -> Result<Option<Page>, String> {
    let client = state.read().await.client.clone();
    // The daemon returns 404 when the page doesn't exist, which reqwest
    // turns into an error. Distinguish "not found" from real errors so the
    // frontend sees None for the former and a real error for the latter —
    // rather than the previous silent `Err(_) => Ok(None)` which hid
    // wrapper/deserialization bugs behind a "not found" UI.
    match client
        .get_json::<GetPageWire>(&format!("/api/pages/{}", id))
        .await
    {
        Ok(wrapped) => Ok(wrapped.page),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("404") || msg.to_lowercase().contains("not found") {
                Ok(None)
            } else {
                Err(format!("get_page failed: {}", msg))
            }
        }
    }
}

#[tauri::command]
pub async fn update_page(
    state: tauri::State<'_, State>,
    id: String,
    content: String,
) -> Result<(), String> {
    let s = state.read().await;
    let req = requests::UpdatePageRequest {
        content,
        source_memory_ids: Vec::new(),
    };
    let _resp: responses::SuccessResponse = s
        .client
        .post_json(&format!("/api/memory/{}/update-page", id), &req)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn archive_page(state: tauri::State<'_, State>, id: String) -> Result<(), String> {
    let s = state.read().await;
    let _resp: serde_json::Value = s
        .client
        .post_empty(&format!("/api/pages/{}/archive", id))
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_page(state: tauri::State<'_, State>, id: String) -> Result<(), String> {
    let s = state.read().await;
    let _resp: serde_json::Value = s.client.delete_path(&format!("/api/pages/{}", id)).await?;
    Ok(())
}

/// Wire wrapper matching the daemon's `{ "pages": [...] }` response shape.
/// Kept local to search.rs because `Page` lives in origin-core (not
/// wenlan-types), so we can't put this in the shared response types.
#[derive(serde::Deserialize)]
struct ListPagesWire {
    pages: Vec<Page>,
}

#[tauri::command]
pub async fn list_pages(
    state: tauri::State<'_, State>,
    status: Option<String>,
    domain: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<Page>, String> {
    let client = state.read().await.client.clone();
    // Build query string from the filter params that were previously ignored.
    let mut params: Vec<String> = Vec::new();
    if let Some(s) = status {
        params.push(format!("status={}", s));
    }
    if let Some(d) = domain {
        params.push(format!("domain={}", d));
    }
    if let Some(l) = limit {
        params.push(format!("limit={}", l));
    }
    if let Some(o) = offset {
        params.push(format!("offset={}", o));
    }
    let path = if params.is_empty() {
        "/api/pages".to_string()
    } else {
        format!("/api/pages?{}", params.join("&"))
    };
    let resp: ListPagesWire = client.get_json(&path).await?;
    Ok(resp.pages)
}

#[tauri::command]
pub async fn search_pages(
    state: tauri::State<'_, State>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<Page>, String> {
    let client = state.read().await.client.clone();
    let req = requests::SearchPagesRequest {
        query,
        limit,
        page_type: None,
    };
    let resp: ListPagesWire = client.post_json("/api/pages/search", &req).await?;
    Ok(resp.pages)
}

#[tauri::command]
pub async fn get_page_sources(
    state: tauri::State<'_, State>,
    page_id: String,
) -> Result<Vec<wenlan_types::PageSourceWithMemory>, String> {
    let client = { state.read().await.client.clone() };
    client.get_page_sources(&page_id).await
}

#[tauri::command]
pub async fn get_page_links(
    state: tauri::State<'_, State>,
    page_id: String,
) -> Result<responses::PageLinksResponse, String> {
    let client = { state.read().await.client.clone() };
    client.get_page_links(&page_id).await
}

#[tauri::command]
pub async fn get_page_revisions(
    state: tauri::State<'_, State>,
    page_id: String,
) -> Result<responses::ListPageRevisionsResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.get_page_revisions(&page_id).await
}

#[tauri::command]
pub async fn list_orphan_links(
    state: tauri::State<'_, State>,
    min_count: Option<usize>,
) -> Result<responses::OrphanLinksResponse, String> {
    let client = { state.read().await.client.clone() };
    client.list_orphan_links(min_count).await
}

#[tauri::command]
pub async fn export_pages_to_obsidian(
    state: tauri::State<'_, State>,
    vault_path: String,
) -> Result<ExportStats, String> {
    // Delegate bulk export to the daemon (POST /api/pages/export).
    // The daemon has direct FS access and owns the ObsidianExporter.
    let client = state.read().await.client.clone();
    let req = requests::ExportPagesRequest {
        vault_path: Some(vault_path),
    };
    client.post_json("/api/pages/export", &req).await
}

#[tauri::command]
pub async fn export_page_to_obsidian(
    state: tauri::State<'_, State>,
    page_id: String,
    vault_path: String,
) -> Result<responses::ExportPageResponse, String> {
    let client = state.read().await.client.clone();
    let path = format!("/api/pages/{}/export", page_id);
    let req = requests::ExportPageRequest { vault_path };
    client.post_json(&path, &req).await
}

#[cfg(test)]
mod export_command_type_tests {
    use super::*;

    #[allow(dead_code)]
    async fn export_page_to_obsidian_uses_daemon_export_response(state: tauri::State<'_, State>) {
        let _: Result<responses::ExportPageResponse, String> =
            export_page_to_obsidian(state, String::new(), String::new()).await;
    }

    #[test]
    fn export_page_to_obsidian_response_type_is_checked() {}
}

#[tauri::command]
pub async fn get_knowledge_path(state: tauri::State<'_, State>) -> Result<String, String> {
    let client = state.read().await.client.clone();
    let resp: responses::KnowledgePathResponse = client.get_json("/api/knowledge/path").await?;
    Ok(resp.path)
}

#[tauri::command]
pub async fn count_knowledge_files(state: tauri::State<'_, State>) -> Result<u64, String> {
    let client = state.read().await.client.clone();
    let resp: responses::KnowledgeCountResponse = client.get_json("/api/knowledge/count").await?;
    Ok(resp.count)
}

// ── Quality gate / rejections ─────────────────────────────────────────

#[tauri::command]
pub async fn get_rejection_log(
    state: tauri::State<'_, State>,
    _limit: Option<usize>,
    _reason: Option<String>,
) -> Result<Vec<RejectionRecord>, String> {
    let s = state.read().await;
    s.client.get_json("/api/memory/rejections").await
}

// ── Decision log ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_decisions_cmd(
    state: tauri::State<'_, State>,
    domain: Option<String>,
    _limit: Option<usize>,
) -> Result<Vec<MemoryItem>, String> {
    let s = state.read().await;
    let mut path = "/api/decisions?limit=200".to_string();
    if let Some(d) = domain {
        path.push_str(&format!("&domain={}", d));
    }
    let resp: responses::DecisionsResponse = s.client.get_json(&path).await?;
    Ok(resp.decisions)
}

#[tauri::command]
pub async fn list_decision_domains_cmd(
    state: tauri::State<'_, State>,
) -> Result<Vec<String>, String> {
    let s = state.read().await;
    let resp: responses::DecisionDomainsResponse =
        s.client.get_json("/api/decisions/domains").await?;
    Ok(resp.domains)
}

// ── Registered source management ──────────────────────────────────────

pub use crate::sources::sync::SyncStats;

#[tauri::command]
pub async fn add_source(
    state: tauri::State<'_, State>,
    watcher: tauri::State<'_, WatcherState>,
    source_type: String,
    path: String,
) -> Result<crate::sources::Source, String> {
    let path_buf = PathBuf::from(&path);
    if !path_buf.exists() {
        return Err(format!("Path does not exist: {}", path));
    }
    if !path_buf.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }

    match source_type.as_str() {
        "obsidian" => {
            // Accept any folder of markdown files, Obsidian vault or not.
            // Frontend detects .obsidian/ for cosmetic badge purposes.
            // `has_any_markdown` short-circuits on the first match instead
            // of walking the entire vault, so very large vaults don't
            // stall the Tauri executor at registration time.
            if !crate::sources::obsidian::has_any_markdown(&path_buf) {
                return Err(format!("No markdown files found in: {}", path));
            }
            let client = {
                let s = state.read().await;
                s.client.clone()
            };
            client.add_source("obsidian".to_string(), path).await
        }
        "directory" => add_directory_source(&state, &watcher, path_buf, &path).await,
        other => Err(format!("Unknown source_type: {}", other)),
    }
}

async fn add_directory_source(
    state: &tauri::State<'_, State>,
    watcher: &tauri::State<'_, WatcherState>,
    path_buf: PathBuf,
    path: &str,
) -> Result<crate::sources::Source, String> {
    let dirname = path_buf
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "dir".to_string());
    let slug = crate::sources::obsidian::slugify(&dirname);
    let id = format!(
        "{}-{}",
        crate::sources::SourceType::Directory.as_str(),
        slug
    );

    let mut cfg = config::load_config();
    if cfg.sources.iter().any(|s| s.path == path_buf) {
        return Err(format!("Source already registered for path: {}", path));
    }

    let source = crate::sources::Source {
        id: id.clone(),
        source_type: crate::sources::SourceType::Directory,
        path: path_buf.clone(),
        status: crate::sources::SyncStatus::Active,
        last_sync: None,
        file_count: 0,
        memory_count: 0,
        last_sync_errors: 0,
        last_sync_error_detail: None,
    };

    cfg.sources.push(source.clone());
    config::save_config(&cfg).map_err(|e| e.to_string())?;

    {
        let mut app_state = state.write().await;
        if !app_state.watch_paths.contains(&path_buf) {
            app_state.watch_paths.push(path_buf.clone());
        }
    }
    let mut watcher_guard = watcher.lock().await;
    if watcher_guard.is_none() {
        let state_arc = state.inner().clone();
        *watcher_guard =
            Some(crate::indexer::create_file_watcher(state_arc).map_err(|e| e.to_string())?);
    }
    if let Some(w) = watcher_guard.as_mut() {
        crate::indexer::watch_path(w, &path_buf).map_err(|e| e.to_string())?;
    }

    Ok(source)
}

#[tauri::command]
pub async fn remove_source(state: tauri::State<'_, State>, id: String) -> Result<(), String> {
    let local_source = config::load_config()
        .sources
        .iter()
        .find(|s| s.id == id)
        .cloned();

    if let Some(source) = local_source {
        if source.source_type == crate::sources::SourceType::Directory {
            return remove_directory_source(&state, &id, source).await;
        }
    }

    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.remove_source(&id).await
}

async fn remove_directory_source(
    state: &tauri::State<'_, State>,
    id: &str,
    source: crate::sources::Source,
) -> Result<(), String> {
    let mut cfg = config::load_config();
    if !cfg.sources.iter().any(|s| s.id == id) {
        return Err(format!("Source not found: {}", id));
    }
    cfg.sources.retain(|s| s.id != id);
    config::save_config(&cfg).map_err(|e| e.to_string())?;

    let mut app_state = state.write().await;
    app_state.watch_paths.retain(|p| p != &source.path);
    Ok(())
}

#[tauri::command]
pub async fn list_registered_sources(
    state: tauri::State<'_, State>,
) -> Result<Vec<crate::sources::Source>, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    let daemon_sources = client.list_sources().await?;
    let local_sources = config::load_config().sources;
    Ok(merge_registered_sources_with_local_directories(
        daemon_sources,
        local_sources,
    ))
}

fn merge_registered_sources_with_local_directories(
    mut daemon_sources: Vec<crate::sources::Source>,
    local_sources: Vec<crate::sources::Source>,
) -> Vec<crate::sources::Source> {
    for source in local_sources
        .into_iter()
        .filter(|s| s.source_type == crate::sources::SourceType::Directory)
    {
        if daemon_sources
            .iter()
            .any(|existing| existing.id == source.id || existing.path == source.path)
        {
            continue;
        }
        daemon_sources.push(source);
    }
    daemon_sources
}

#[tauri::command]
pub async fn sync_registered_source(
    state: tauri::State<'_, State>,
    id: String,
) -> Result<SyncStats, String> {
    let local_source = config::load_config()
        .sources
        .iter()
        .find(|s| s.id == id)
        .cloned();

    if matches!(
        local_source.as_ref().map(|s| &s.source_type),
        Some(crate::sources::SourceType::Directory)
    ) {
        return Err("Only Obsidian sources support manual sync; directory sources use the live file watcher".to_string());
    }

    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    let stats = client.sync_source(&id).await?;
    Ok(SyncStats {
        files_found: stats.files_found,
        ingested: stats.ingested,
        skipped: stats.skipped,
        errors: stats.errors,
        error_detail: None,
    })
}

// ---------------------------------------------------------------------------
// External LLM provider commands (Ollama, LM Studio, etc.)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_model_choice(
    state: tauri::State<'_, State>,
) -> Result<(Option<String>, Option<String>), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.get_model_choice().await
}

#[tauri::command]
pub async fn set_model_choice(
    state: tauri::State<'_, State>,
    routine_model: Option<String>,
    synthesis_model: Option<String>,
) -> Result<(), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client
        .set_model_choice(routine_model, synthesis_model)
        .await?;
    log::info!("[settings] Model choice updated — restart daemon to apply");
    Ok(())
}

#[tauri::command]
pub async fn get_system_info() -> Result<wenlan_types::system_info::SystemInfo, String> {
    Ok(crate::system_info::detect_system_info())
}

#[tauri::command]
pub async fn get_external_llm(
    state: tauri::State<'_, State>,
) -> Result<(Option<String>, Option<String>), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.get_external_llm().await
}

#[tauri::command]
pub async fn set_external_llm(
    state: tauri::State<'_, State>,
    endpoint: Option<String>,
    model: Option<String>,
) -> Result<(), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.set_external_llm(endpoint, model).await?;
    log::info!("[settings] External LLM config updated — restart daemon to apply");
    Ok(())
}

#[tauri::command]
pub async fn test_external_llm(
    state: tauri::State<'_, State>,
    endpoint: String,
    model: String,
) -> Result<requests::TestLlmResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.test_llm(endpoint, model).await
}

#[cfg(test)]
mod external_llm_command_type_tests {
    use super::*;

    #[allow(dead_code)]
    async fn test_external_llm_uses_daemon_response_envelope(state: tauri::State<'_, State>) {
        let _: Result<requests::TestLlmResponse, String> =
            test_external_llm(state, String::new(), String::new()).await;
    }

    #[test]
    fn test_external_llm_response_type_is_checked() {}
}

/// Proxy for `GET /api/on-device-model` — returns per-model cache/load state.
#[tauri::command]
pub async fn get_on_device_model(
    state: tauri::State<'_, State>,
) -> Result<crate::api::OnDeviceModelResponse, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.get_on_device_model().await
}

/// Proxy for `POST /api/on-device-model/download` — triggers download + hot-load.
/// This is a long-running call (minutes for a 2.7GB download).
#[tauri::command]
pub async fn download_on_device_model(
    state: tauri::State<'_, State>,
    model_id: String,
) -> Result<(), String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.download_on_device_model(model_id).await
}

#[cfg(test)]
mod on_device_model_command_type_tests {
    use super::*;

    #[allow(dead_code)]
    async fn get_on_device_model_uses_typed_response(state: tauri::State<'_, State>) {
        let _: Result<crate::api::OnDeviceModelResponse, String> = get_on_device_model(state).await;
    }

    #[test]
    fn on_device_model_command_response_type_is_checked() {}
}

// ── Home delta feed ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_recent_retrievals(
    state: tauri::State<'_, State>,
    limit: Option<i64>,
) -> Result<Vec<wenlan_types::RetrievalEvent>, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.list_recent_retrievals(limit.unwrap_or(10)).await
}

#[tauri::command]
pub async fn list_recent_changes(
    state: tauri::State<'_, State>,
    limit: Option<i64>,
) -> Result<Vec<wenlan_types::PageChange>, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.list_recent_changes(limit.unwrap_or(10)).await
}

#[tauri::command]
pub async fn list_recent_memories(
    state: tauri::State<'_, State>,
    limit: Option<i64>,
    since_ms: Option<i64>,
) -> Result<Vec<wenlan_types::RecentActivityItem>, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client
        .list_recent_memories(limit.unwrap_or(10), since_ms)
        .await
}

#[tauri::command]
pub async fn list_unconfirmed_memories(
    state: tauri::State<'_, State>,
    limit: Option<i64>,
) -> Result<Vec<wenlan_types::RecentActivityItem>, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.list_unconfirmed_memories(limit.unwrap_or(6)).await
}

#[tauri::command]
pub async fn list_recent_pages(
    state: tauri::State<'_, State>,
    limit: Option<i64>,
    since_ms: Option<i64>,
) -> Result<Vec<wenlan_types::RecentActivityItem>, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client
        .list_recent_pages(limit.unwrap_or(10), since_ms)
        .await
}

#[tauri::command]
pub async fn list_recent_relations(
    state: tauri::State<'_, State>,
    limit: Option<usize>,
    since_ms: Option<i64>,
) -> Result<Vec<wenlan_types::RecentRelation>, String> {
    let client = {
        let s = state.read().await;
        s.client.clone()
    };
    client.list_recent_relations(limit, since_ms).await
}

// ── Lifecycle commands ─────────────────────────────────────────────────────

#[tauri::command]
pub async fn is_run_at_login_enabled() -> Result<bool, String> {
    use crate::lifecycle::{is_run_at_login_enabled as inner, SystemLaunchctl};
    Ok(inner(&SystemLaunchctl))
}

#[tauri::command]
pub async fn set_run_at_login(enabled: bool) -> Result<(), String> {
    use crate::lifecycle::{set_run_at_login as inner, SystemLaunchctl};
    inner(enabled, &SystemLaunchctl)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn quit_wenlan_full(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::lifecycle::quit_origin(&app_handle)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn quit_origin_full(app_handle: tauri::AppHandle) -> Result<(), String> {
    quit_wenlan_full(app_handle).await
}

#[cfg(test)]
mod lifecycle_command_tests {
    use super::*;

    #[test]
    fn exposes_wenlan_and_legacy_quit_commands() {
        let _quit_wenlan = quit_wenlan_full;
        let _quit_origin = quit_origin_full;
    }
}

#[cfg(test)]
mod status_response_tests {
    use super::*;

    #[test]
    fn daemon_status_updates_count_and_reranker_without_forcing_indexing_state() {
        let local = IndexStatus {
            is_running: false,
            files_indexed: 3,
            files_total: 7,
            last_error: Some("local watcher error".to_string()),
            sources_connected: vec!["local".to_string()],
            reranker: wenlan_types::responses::RerankerStatus::Disabled,
            reranker_light: wenlan_types::responses::RerankerStatus::Disabled,
            reranker_mode: "off".to_string(),
        };
        let daemon = responses::StatusResponse {
            is_running: true,
            files_indexed: 42,
            files_total: 0,
            sources_connected: vec!["daemon".to_string()],
            reranker: wenlan_types::responses::RerankerStatus::Failed {
                reason: "model missing".to_string(),
            },
            reranker_light: wenlan_types::responses::RerankerStatus::Active {
                model_id: "bge-reranker".to_string(),
            },
            reranker_mode: "lite".to_string(),
        };

        let merged = merge_daemon_status(local, daemon);

        assert!(!merged.is_running);
        assert_eq!(merged.files_indexed, 42);
        assert_eq!(merged.files_total, 7);
        assert_eq!(merged.last_error.as_deref(), Some("local watcher error"));
        assert_eq!(merged.sources_connected, vec!["daemon".to_string()]);
        assert_eq!(merged.reranker_mode, "lite");
        assert_eq!(
            merged.reranker,
            wenlan_types::responses::RerankerStatus::Failed {
                reason: "model missing".to_string()
            }
        );
        assert_eq!(
            merged.reranker_light,
            wenlan_types::responses::RerankerStatus::Active {
                model_id: "bge-reranker".to_string()
            }
        );
    }
}

#[cfg(test)]
mod tag_data_tests {
    use super::*;

    #[test]
    fn set_document_tags_request_serializes_source_and_tags() {
        let request = SetDocumentTagsRequest {
            source: "manual".to_string(),
            tags: vec!["rust".to_string()],
        };

        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["source"], "manual");
        assert_eq!(value["tags"], serde_json::json!(["rust"]));
    }

    #[test]
    fn tag_data_from_inventory_preserves_document_tags() {
        let mut document_tags = HashMap::new();
        document_tags.insert("memory::mem1".to_string(), vec!["rust".to_string()]);

        let tag_data = tag_data_from_inventory(crate::api::TagInventoryResponse {
            tags: vec!["rust".to_string()],
            document_tags,
        });

        assert_eq!(tag_data.tags, vec!["rust"]);
        assert_eq!(
            tag_data.document_tags.get("memory::mem1"),
            Some(&vec!["rust".to_string()])
        );
        assert!(tag_data.categories.is_empty());
        assert!(tag_data.document_categories.is_empty());
    }
}

#[cfg(test)]
mod avatar_path_tests {
    use super::*;
    use std::ffi::OsString;

    fn restore_env(key: &str, previous: Option<OsString>) {
        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    #[serial_test::serial]
    fn avatar_storage_dir_prefers_wenlan_data_dir() {
        let previous_wenlan = std::env::var_os("WENLAN_DATA_DIR");
        let previous_origin = std::env::var_os("ORIGIN_DATA_DIR");
        let tmp = tempfile::tempdir().unwrap();

        std::env::set_var("WENLAN_DATA_DIR", tmp.path());
        std::env::set_var("ORIGIN_DATA_DIR", "/tmp/legacy-origin-avatar-root");

        assert_eq!(avatar_storage_dir(), tmp.path().join("avatars"));

        restore_env("WENLAN_DATA_DIR", previous_wenlan);
        restore_env("ORIGIN_DATA_DIR", previous_origin);
    }

    #[test]
    #[serial_test::serial]
    fn avatar_storage_dir_prefers_wenlan_data_dir_when_both_are_set() {
        let previous_wenlan = std::env::var_os("WENLAN_DATA_DIR");
        let previous_origin = std::env::var_os("ORIGIN_DATA_DIR");
        let current = tempfile::tempdir().unwrap();
        let legacy = tempfile::tempdir().unwrap();

        std::env::set_var("WENLAN_DATA_DIR", current.path());
        std::env::set_var("ORIGIN_DATA_DIR", legacy.path());

        assert_eq!(avatar_storage_dir(), current.path().join("avatars"));

        restore_env("WENLAN_DATA_DIR", previous_wenlan);
        restore_env("ORIGIN_DATA_DIR", previous_origin);
    }

    #[test]
    #[serial_test::serial]
    fn avatar_storage_dir_falls_back_to_legacy_origin_data_dir() {
        let previous_wenlan = std::env::var_os("WENLAN_DATA_DIR");
        let previous_origin = std::env::var_os("ORIGIN_DATA_DIR");
        let tmp = tempfile::tempdir().unwrap();

        std::env::remove_var("WENLAN_DATA_DIR");
        std::env::set_var("ORIGIN_DATA_DIR", tmp.path());

        assert_eq!(avatar_storage_dir(), tmp.path().join("avatars"));

        restore_env("WENLAN_DATA_DIR", previous_wenlan);
        restore_env("ORIGIN_DATA_DIR", previous_origin);
    }

    #[test]
    #[serial_test::serial]
    fn avatar_storage_dir_uses_legacy_default_when_current_empty_and_legacy_has_avatars() {
        let previous_home = std::env::var_os("HOME");
        let previous_wenlan = std::env::var_os("WENLAN_DATA_DIR");
        let previous_origin = std::env::var_os("ORIGIN_DATA_DIR");
        let tmp = tempfile::tempdir().unwrap();

        std::env::set_var("HOME", tmp.path());
        std::env::remove_var("WENLAN_DATA_DIR");
        std::env::remove_var("ORIGIN_DATA_DIR");

        let current = dirs::data_local_dir().unwrap().join("wenlan");
        let legacy = dirs::data_local_dir().unwrap().join("origin");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(legacy.join("avatars")).unwrap();

        assert_eq!(avatar_storage_dir(), legacy.join("avatars"));

        restore_env("HOME", previous_home);
        restore_env("WENLAN_DATA_DIR", previous_wenlan);
        restore_env("ORIGIN_DATA_DIR", previous_origin);
    }

    #[test]
    #[serial_test::serial]
    fn resolves_missing_legacy_avatar_to_wenlan_copy() {
        let previous_wenlan = std::env::var_os("WENLAN_DATA_DIR");
        let previous_origin = std::env::var_os("ORIGIN_DATA_DIR");
        let current = tempfile::tempdir().unwrap();
        let legacy = tempfile::tempdir().unwrap();
        let filename = "57515813-4419-4116-bea6-21bc66e1a511.jpg";

        std::env::set_var("WENLAN_DATA_DIR", current.path());
        std::env::set_var("ORIGIN_DATA_DIR", legacy.path());
        std::fs::create_dir_all(current.path().join("avatars")).unwrap();
        std::fs::write(current.path().join("avatars").join(filename), b"avatar").unwrap();

        let legacy_path = legacy.path().join("avatars").join(filename);

        assert_eq!(
            resolve_profile_avatar_path(Some(legacy_path.to_string_lossy().to_string())),
            Some(
                current
                    .path()
                    .join("avatars")
                    .join(filename)
                    .to_string_lossy()
                    .to_string()
            )
        );

        restore_env("WENLAN_DATA_DIR", previous_wenlan);
        restore_env("ORIGIN_DATA_DIR", previous_origin);
    }

    #[test]
    #[serial_test::serial]
    fn does_not_resolve_arbitrary_missing_path_to_avatar_copy() {
        let previous_wenlan = std::env::var_os("WENLAN_DATA_DIR");
        let previous_origin = std::env::var_os("ORIGIN_DATA_DIR");
        let current = tempfile::tempdir().unwrap();
        let filename = "same-name.jpg";

        std::env::set_var("WENLAN_DATA_DIR", current.path());
        std::env::remove_var("ORIGIN_DATA_DIR");
        std::fs::create_dir_all(current.path().join("avatars")).unwrap();
        std::fs::write(current.path().join("avatars").join(filename), b"avatar").unwrap();

        let arbitrary_path = current.path().join("downloads").join(filename);

        assert_eq!(
            resolve_profile_avatar_path(Some(arbitrary_path.to_string_lossy().to_string())),
            None
        );

        restore_env("WENLAN_DATA_DIR", previous_wenlan);
        restore_env("ORIGIN_DATA_DIR", previous_origin);
    }

    #[test]
    #[serial_test::serial]
    fn does_not_resolve_non_origin_avatar_dir_to_wenlan_copy() {
        let previous_wenlan = std::env::var_os("WENLAN_DATA_DIR");
        let previous_origin = std::env::var_os("ORIGIN_DATA_DIR");
        let current = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let filename = "same-name.jpg";

        std::env::set_var("WENLAN_DATA_DIR", current.path());
        std::env::remove_var("ORIGIN_DATA_DIR");
        std::fs::create_dir_all(current.path().join("avatars")).unwrap();
        std::fs::write(current.path().join("avatars").join(filename), b"avatar").unwrap();

        let non_origin_avatar_path = other
            .path()
            .join("not-origin")
            .join("avatars")
            .join(filename);

        assert_eq!(
            resolve_profile_avatar_path(Some(non_origin_avatar_path.to_string_lossy().to_string())),
            None
        );

        restore_env("WENLAN_DATA_DIR", previous_wenlan);
        restore_env("ORIGIN_DATA_DIR", previous_origin);
    }
}

#[cfg(test)]
mod revision_response_tests {
    use super::*;

    #[test]
    fn store_memory_response_preserves_revision_signals() {
        let response = StoreMemoryResponse {
            source_id: "mem_new".to_string(),
            warnings: vec![],
            enrichment: "pending".to_string(),
            hint: "Review the protected memory update.".to_string(),
            triggered_revisions: vec!["mem_target".to_string()],
            auto_superseded: vec!["mem_old".to_string()],
        };

        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(
            json["triggered_revisions"],
            serde_json::json!(["mem_target"])
        );
        assert_eq!(json["auto_superseded"], serde_json::json!(["mem_old"]));

        let decoded: StoreMemoryResponse = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.triggered_revisions, vec!["mem_target"]);
        assert_eq!(decoded.auto_superseded, vec!["mem_old"]);
    }

    #[test]
    fn store_memory_response_defaults_revision_signals_for_old_daemons() {
        let decoded: StoreMemoryResponse = serde_json::from_value(serde_json::json!({
            "source_id": "mem_new",
            "warnings": [],
            "enrichment": "not_needed",
            "hint": ""
        }))
        .unwrap();

        assert!(decoded.triggered_revisions.is_empty());
        assert!(decoded.auto_superseded.is_empty());
    }
}

#[cfg(test)]
mod registered_source_tests {
    use super::*;

    fn source(
        id: &str,
        source_type: crate::sources::SourceType,
        path: &str,
    ) -> crate::sources::Source {
        crate::sources::Source {
            id: id.to_string(),
            source_type,
            path: PathBuf::from(path),
            status: crate::sources::SyncStatus::Active,
            last_sync: None,
            file_count: 0,
            memory_count: 0,
            last_sync_errors: 0,
            last_sync_error_detail: None,
        }
    }

    #[test]
    fn registered_source_listing_keeps_local_directory_sources_only() {
        let daemon_sources = vec![source(
            "obsidian-daemon",
            crate::sources::SourceType::Obsidian,
            "/Users/test/vault",
        )];
        let local_sources = vec![
            source(
                "directory-local",
                crate::sources::SourceType::Directory,
                "/Users/test/docs",
            ),
            source(
                "obsidian-stale-local",
                crate::sources::SourceType::Obsidian,
                "/Users/test/old-vault",
            ),
        ];

        let merged = merge_registered_sources_with_local_directories(daemon_sources, local_sources);

        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|s| s.id == "obsidian-daemon"));
        assert!(merged.iter().any(|s| s.id == "directory-local"));
        assert!(!merged.iter().any(|s| s.id == "obsidian-stale-local"));
    }
}
