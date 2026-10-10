// SPDX-License-Identifier: AGPL-3.0-only
//! `wenlan://pair?code=<pairing code>` links from the relay pairing page.
//!
//! A link only raises the window and opens the approval dialog. Allowing still
//! takes an explicit click there, so a link someone else sends can never grant
//! access on its own.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime, Url};

/// Frontend event: a pairing link is waiting in [`PendingPairingLink`].
pub const PAIRING_LINK_EVENT: &str = "remote-pairing-link";

/// The most recent valid link, held until the main window takes it. A cold
/// start delivers the link before the frontend is listening, so the frontend
/// pulls it with [`take_remote_pairing_link`] instead of relying on the event.
#[derive(Default)]
pub struct PendingPairingLink(Mutex<Option<String>>);

impl PendingPairingLink {
    fn put(&self, code: String) {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(code);
    }

    /// Hands the code out once, so a remount or a repeated event cannot reopen it.
    fn take(&self) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }
}

/// Returns the pairing code when `url` is exactly `wenlan://pair?code=<64 chars>`.
/// Anything else, including extra parameters, is ignored.
pub fn pairing_code(url: &Url) -> Option<String> {
    if url.scheme() != "wenlan"
        || url.host_str() != Some("pair")
        || !matches!(url.path(), "" | "/")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut pairs = url.query_pairs();
    let (key, code) = pairs.next()?;
    if key != "code" || pairs.next().is_some() || !valid_code(&code) {
        return None;
    }
    Some(code.into_owned())
}

/// Same shape the Connections screen accepts for a pasted code.
fn valid_code(code: &str) -> bool {
    code.len() == 64
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Handles URLs the OS opened the app with. The first valid pairing link wins.
pub fn accept_urls<R: Runtime>(app: &AppHandle<R>, urls: &[Url]) {
    let Some(code) = urls.iter().find_map(pairing_code) else {
        if !urls.is_empty() {
            // Never log the URL itself: a pairing code is a short-lived capability.
            log::warn!(
                "[pairing-link] ignored {} link(s) that are not wenlan://pair?code=",
                urls.len()
            );
        }
        return;
    };
    if let Some(pending) = app.try_state::<PendingPairingLink>() {
        pending.put(code);
    }
    crate::reveal_main_window_on_main_thread(app);
    if let Err(error) = app.emit_to("main", PAIRING_LINK_EVENT, ()) {
        log::warn!("[pairing-link] could not notify the main window: {error}");
    }
}

#[tauri::command]
pub fn take_remote_pairing_link(pending: tauri::State<'_, PendingPairingLink>) -> Option<String> {
    pending.take()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_";

    fn parse(raw: &str) -> Option<String> {
        pairing_code(&Url::parse(raw).unwrap())
    }

    #[test]
    fn a_link_is_handed_out_once() {
        let pending = PendingPairingLink::default();
        assert_eq!(pending.take(), None);
        pending.put(CODE.to_string());
        pending.put(CODE.replace('a', "b"));
        assert_eq!(pending.take(), Some(CODE.replace('a', "b")));
        assert_eq!(pending.take(), None);
    }

    #[test]
    fn accepts_the_relay_link_shape() {
        assert_eq!(CODE.len(), 64);
        assert_eq!(
            parse(&format!("wenlan://pair?code={CODE}")).as_deref(),
            Some(CODE)
        );
        assert_eq!(
            parse(&format!("wenlan://pair/?code={CODE}")).as_deref(),
            Some(CODE)
        );
    }

    #[test]
    fn rejects_every_other_shape() {
        for raw in [
            format!("https://pair?code={CODE}"),
            format!("wenlan://approve?code={CODE}"),
            format!("wenlan://pair/extra?code={CODE}"),
            format!("wenlan://pair?code={CODE}&approve=1"),
            format!("wenlan://pair?approve=1&code={CODE}"),
            format!("wenlan://pair?token={CODE}"),
            format!("wenlan://pair?code={CODE}#x"),
            format!("wenlan://user@pair?code={CODE}"),
            format!("wenlan://pair:9?code={CODE}"),
            format!("wenlan://pair?code={}", &CODE[1..]),
            format!("wenlan://pair?code={CODE}a"),
            format!("wenlan://pair?code={}%21", &CODE[1..]),
            "wenlan://pair".to_string(),
            "wenlan://pair?code=".to_string(),
        ] {
            assert_eq!(parse(&raw), None, "{raw}");
        }
    }
}
