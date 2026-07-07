// SPDX-License-Identifier: Apache-2.0
//! Cross-origin request guard for the loopback daemon.
//!
//! The daemon has no auth and binds `127.0.0.1:7878` by default, so without a
//! guard any web page the user visits could drive it — cross-origin reads of
//! the whole memory store, CSRF writes, or DNS-rebinding. Native clients (the
//! desktop app, the `wenlan` CLI, `wenlan-mcp`) all talk to the daemon over
//! reqwest and send no `Origin` header, so they pass through untouched.
//! Browsers always attach `Origin` on cross-origin requests; a non-local one
//! is rejected. A non-local `Host` (DNS rebinding) is rejected too — unless the
//! operator deliberately exposed the daemon via `WENLAN_BIND_ADDR` (e.g. the
//! Docker image), which opts out of the Host check and owns its own access
//! control.

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};

/// Reject browser-driven cross-origin requests before they reach a handler.
pub async fn guard_local_only(req: Request, next: Next) -> Result<Response, StatusCode> {
    let headers = req.headers();

    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        if !origin_is_local(origin) {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // DNS-rebinding defense applies only to the default loopback bind. When the
    // operator sets WENLAN_BIND_ADDR (Docker/LAN), the Host is legitimately
    // non-loopback and access control is their responsibility.
    if wenlan_core::env_compat::var_compat("WENLAN_BIND_ADDR").is_none() {
        if let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
            if !host_is_local(host) {
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }

    Ok(next.run(req).await)
}

/// True for `localhost` / `127.0.0.1` / `::1`, with or without a `:port`.
fn host_is_local(host: &str) -> bool {
    let hostname = if let Some(rest) = host.strip_prefix('[') {
        // Bracketed IPv6: "[::1]" or "[::1]:7878" — take up to the closing ']'.
        rest.split(']').next().unwrap_or(rest)
    } else if host.matches(':').count() == 1 {
        // "host:port" — strip the port (a single colon can't be bare IPv6).
        host.split(':').next().unwrap_or(host)
    } else {
        // Bare hostname / IPv4, or a bare IPv6 literal like "::1" (2+ colons).
        host
    };
    matches!(hostname, "localhost" | "127.0.0.1" | "::1")
}

/// True for a local `Origin` header value (or the Tauri webview origins).
pub(crate) fn origin_is_local(origin: &str) -> bool {
    if origin == "tauri://localhost" || origin == "http://tauri.localhost" {
        return true;
    }
    let after_scheme = origin
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(origin);
    host_is_local(after_scheme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_local_variants() {
        for h in [
            "127.0.0.1",
            "127.0.0.1:7878",
            "localhost",
            "localhost:7878",
            "::1",
            "[::1]",
            "[::1]:7878",
        ] {
            assert!(host_is_local(h), "expected local: {h}");
        }
    }

    #[test]
    fn host_non_local_rejected() {
        for h in [
            "evil.com",
            "evil.com:7878",
            "192.168.1.5:7878",
            "wenlan.evil.com",
        ] {
            assert!(!host_is_local(h), "expected non-local: {h}");
        }
    }

    #[test]
    fn origin_local_and_tauri_allowed() {
        for o in [
            "http://localhost:1420",
            "http://127.0.0.1:7878",
            "https://localhost",
            "tauri://localhost",
            "http://tauri.localhost",
        ] {
            assert!(origin_is_local(o), "expected local origin: {o}");
        }
    }

    #[test]
    fn origin_cross_site_and_null_rejected() {
        for o in ["https://evil.com", "http://attacker.test:1420", "null"] {
            assert!(!origin_is_local(o), "expected rejected origin: {o}");
        }
    }
}
