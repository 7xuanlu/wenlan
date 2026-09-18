// SPDX-License-Identifier: AGPL-3.0-only
//! Bounded native disconnect coordination.

use super::store::{Profile, Store, StoreError};
use super::RelayClient;

/// The local state observed while preparing a disconnect.
///
/// This deliberately has no `Debug` or `Serialize` implementation. The
/// profile may contain the device management credential needed for revocation.
pub(crate) struct StopPlan {
    profile: Option<Profile>,
    persistence_error: Option<StoreError>,
}

impl StopPlan {
    fn new(profile: Option<Profile>, persistence_error: Option<StoreError>) -> Self {
        Self {
            profile,
            persistence_error,
        }
    }
}

/// Load the current native profile and persist stop intent before revocation.
///
/// A stale caller never receives a loaded profile in the plan: doing so could
/// revoke a device belonging to a newer profile. When disabling fails, the
/// original loaded profile is retained so its device can still be revoked.
pub(crate) fn prepare(store: &Store, expected_revision: Option<&str>) -> StopPlan {
    prepare_with_disable(store, expected_revision, |store, revision| {
        store.disable(revision)
    })
}

fn prepare_with_disable(
    store: &Store,
    expected_revision: Option<&str>,
    disable: impl FnOnce(&Store, &str) -> Result<Profile, StoreError>,
) -> StopPlan {
    let profile = match store.load() {
        Ok(Some(profile)) => profile,
        Ok(None) => return StopPlan::new(None, None),
        Err(error) => return StopPlan::new(None, Some(error)),
    };

    if expected_revision.is_some_and(|expected| expected != profile.revision()) {
        return StopPlan::new(None, Some(StoreError::Stale));
    }

    if !profile.enabled() {
        return StopPlan::new(Some(profile), None);
    }

    let revision = profile.revision().to_string();
    match disable(store, &revision) {
        Ok(disabled) => StopPlan::new(Some(disabled), None),
        Err(error) => StopPlan::new(Some(profile), Some(error)),
    }
}

enum RevokeOutcome {
    NotNeeded,
    Succeeded,
    Failed(String),
}

/// Revoke the retained device, then clear local credentials only with a CAS.
///
/// There are no retries here. In particular, a failed persistence operation
/// still gets its one revoke attempt, but can never be followed by local
/// credential deletion because the disabled revision was not confirmed.
pub(crate) async fn finish(
    store: Store,
    client: RelayClient,
    plan: StopPlan,
) -> Result<(), String> {
    let StopPlan {
        profile,
        persistence_error,
    } = plan;
    let Some(profile) = profile else {
        return persistence_error.map_or(Ok(()), |error| {
            Err(format!(
                "Local transport stop requested; remote access settings are not confirmed ({error}); server revoke was not attempted because no device snapshot was retained; disconnect must be retried before app restart"
            ))
        });
    };

    let device = profile.device().cloned();
    let revoke = match device.as_ref() {
        Some(device) => match client.revoke_device(device).await {
            Ok(()) => RevokeOutcome::Succeeded,
            Err(error) => RevokeOutcome::Failed(error.to_string()),
        },
        None => RevokeOutcome::NotNeeded,
    };

    match persistence_error {
        Some(persistence_error) => {
            let outcome = match revoke {
                RevokeOutcome::Succeeded => "server revoke succeeded".to_string(),
                RevokeOutcome::Failed(error) => format!("server revoke failed: {error}"),
                RevokeOutcome::NotNeeded => "no server device revoke was needed".to_string(),
            };
            Err(format!(
                "Local transport stop requested; remote access settings are not confirmed ({persistence_error}); {outcome}; disconnect must be retried before app restart"
            ))
        }
        None => match revoke {
            RevokeOutcome::Failed(error) => Err(error),
            RevokeOutcome::NotNeeded => Ok(()),
            RevokeOutcome::Succeeded => {
                let Some(device) = device else {
                    return Ok(());
                };
                let revision = profile.revision().to_string();
                let device_id = device.id;
                tokio::task::spawn_blocking(move || store.finish_disconnect(&revision, &device_id))
                    .await
                    .map_err(|_| "Remote access storage task failed".to_string())?
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_relay::{now_ms, DeviceCredential};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn configured() -> (tempfile::TempDir, Store, Profile) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::in_directory(directory.path().join("relay"));
        let profile = store.configure(None, "review").unwrap();
        let profile = store.enable(profile.revision()).unwrap();
        (directory, store, profile)
    }

    fn device() -> DeviceCredential {
        DeviceCredential {
            id: "d".repeat(64),
            management_token: "m".repeat(64),
            expires_at: now_ms() + 60_000,
        }
    }

    async fn relay_server(
        status: u16,
        body: &str,
    ) -> (RelayClient, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = body.to_string();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut buffer = [0; 2048];
                let length = socket.read(&mut buffer).await.unwrap();
                assert!(length > 0);
                request.extend_from_slice(&buffer[..length]);
                if request.windows(4).any(|part| part == b"\r\n\r\n") {
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8(request).unwrap()
        });
        let client = RelayClient::build(
            reqwest::Url::parse(&format!("http://127.0.0.1:{port}")).unwrap(),
            false,
        )
        .unwrap();
        (client, task)
    }

    #[test]
    fn missing_profile_is_a_no_op() {
        let (_directory, store) = {
            let directory = tempfile::tempdir().unwrap();
            let store = Store::in_directory(directory.path().join("relay"));
            (directory, store)
        };
        let plan = prepare(&store, None);
        assert!(plan.profile.is_none());
        assert!(plan.persistence_error.is_none());
    }

    #[test]
    fn stale_profile_does_not_retain_a_device_for_revoke() {
        let (_directory, store, profile) = configured();
        let enrolled = store.attach_device(profile.revision(), device()).unwrap();
        let plan = prepare(&store, Some("stale-revision"));
        assert!(plan.profile.is_none());
        assert_eq!(plan.persistence_error, Some(StoreError::Stale));
        let saved = store.load().unwrap().unwrap();
        assert_eq!(saved.revision(), enrolled.revision());
        assert!(saved.device().is_some());
    }

    #[tokio::test]
    async fn persistence_failure_without_a_device_returns_an_explicit_warning() {
        let (_directory, store, profile) = configured();
        let plan = prepare_with_disable(&store, Some(profile.revision()), |_store, _revision| {
            Err(StoreError::Storage)
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = RelayClient::build(
            reqwest::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap(),
            false,
        )
        .unwrap();
        let error = finish(store.clone(), client, plan).await.unwrap_err();
        assert!(error.contains("Local transport stop requested"));
        assert!(error.contains("settings are not confirmed"));
        assert!(error.contains("no server device revoke was needed"));
        assert!(error.contains("retried before app restart"));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
        let saved = store.load().unwrap().unwrap();
        assert!(saved.enabled());
        assert!(saved.device().is_none());
    }

    #[test]
    fn load_failure_does_not_retain_a_device_for_revoke() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("relay");
        std::fs::write(&path, b"not-a-directory").unwrap();
        let store = Store::in_directory(path);
        let plan = prepare(&store, None);
        assert!(plan.profile.is_none());
        assert_eq!(plan.persistence_error, Some(StoreError::Storage));
    }

    #[tokio::test]
    async fn busy_snapshot_read_never_attempts_revoke_and_preserves_credentials() {
        let (directory, store, profile) = configured();
        let enrolled = store.attach_device(profile.revision(), device()).unwrap();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(directory.path().join("relay/connection.lock"))
            .unwrap();
        lock.lock().unwrap();

        let plan = prepare_with_disable(&store, None, |_, _| {
            panic!("disable must not run without a loaded snapshot")
        });
        assert!(plan.profile.is_none());
        assert_eq!(plan.persistence_error, Some(StoreError::Busy));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = RelayClient::build(
            reqwest::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap(),
            false,
        )
        .unwrap();
        let error = finish(store.clone(), client, plan).await.unwrap_err();
        assert!(error.contains("storage is busy"), "{error}");
        assert!(error.contains("no device snapshot was retained"), "{error}");
        assert!(!error.contains("server revoke failed"));
        assert!(!error.contains("server revoke succeeded"));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
        drop(lock);

        let restored = store.load().unwrap().unwrap();
        assert!(restored.enabled());
        assert_eq!(restored.revision(), enrolled.revision());
        assert_eq!(
            restored.device().unwrap().management_token,
            enrolled.device().unwrap().management_token
        );
        let retry = prepare_with_disable(&store, None, |_, _| Err(StoreError::Storage));
        assert!(retry.profile.as_ref().and_then(Profile::device).is_some());
        assert_eq!(retry.persistence_error, Some(StoreError::Storage));
    }

    #[test]
    fn disable_failure_retains_the_original_profile_for_revoke() {
        let (_directory, store, profile) = configured();
        let enrolled = store.attach_device(profile.revision(), device()).unwrap();
        let plan = prepare_with_disable(&store, None, |_store, _revision| Err(StoreError::Storage));
        assert_eq!(
            plan.profile.as_ref().unwrap().revision(),
            enrolled.revision()
        );
        assert_eq!(
            plan.profile.as_ref().unwrap().device().unwrap().id,
            enrolled.device().unwrap().id
        );
        assert_eq!(plan.persistence_error, Some(StoreError::Storage));
    }

    #[tokio::test]
    async fn persistence_failure_still_revokes_and_retains_local_credentials() {
        let (_directory, store, profile) = configured();
        let enrolled = store.attach_device(profile.revision(), device()).unwrap();
        let plan = prepare_with_disable(&store, None, |_store, _revision| Err(StoreError::Storage));
        let (client, task) = relay_server(200, r#"{"success":true}"#).await;
        let error = finish(store.clone(), client, plan).await.unwrap_err();
        assert!(error.contains("Local transport stop requested"));
        assert!(error.contains("settings are not confirmed"));
        assert!(error.contains("server revoke succeeded"));
        assert!(error.contains("retried before app restart"));
        let request = task.await.unwrap();
        assert!(request.starts_with("POST /devices/revoke HTTP/1.1"));
        let saved = store.load().unwrap().unwrap();
        assert!(saved.enabled());
        assert_eq!(saved.revision(), enrolled.revision());
        assert!(saved.device().is_some());
    }

    #[tokio::test]
    async fn simultaneous_storage_and_revoke_failure_is_not_reported_as_durable_stop() {
        let (_directory, store, profile) = configured();
        let enrolled = store.attach_device(profile.revision(), device()).unwrap();
        let plan = prepare_with_disable(&store, None, |_store, _revision| Err(StoreError::Storage));
        assert!(
            plan.profile.as_ref().and_then(Profile::device).is_some(),
            "device snapshot missing: {:?}",
            plan.persistence_error
        );
        let (client, task) = relay_server(503, "PRIVATE_ERROR").await;
        let error = finish(store.clone(), client, plan).await.unwrap_err();
        assert!(error.contains("transport stop requested"));
        assert!(error.contains("settings are not confirmed"));
        assert!(error.contains("server revoke failed"), "{error}");
        assert!(error.contains("retried before app restart"));
        assert!(!error.contains("PRIVATE_ERROR"));
        assert!(!error.contains(&enrolled.device().unwrap().management_token));
        task.await.unwrap();
        let saved = store.load().unwrap().unwrap();
        assert!(saved.enabled());
        assert_eq!(saved.revision(), enrolled.revision());
        assert!(saved.device().is_some());
    }

    #[tokio::test]
    async fn remote_failure_retains_disabled_credentials() {
        let (_directory, store, profile) = configured();
        let enrolled = store.attach_device(profile.revision(), device()).unwrap();
        let plan = prepare(&store, Some(enrolled.revision()));
        let disabled = plan.profile.as_ref().unwrap().clone();
        let (client, task) = relay_server(401, "{}").await;
        assert!(finish(store.clone(), client, plan).await.is_err());
        assert!(task
            .await
            .unwrap()
            .starts_with("POST /devices/revoke HTTP/1.1"));
        let saved = store.load().unwrap().unwrap();
        assert!(!saved.enabled());
        assert_eq!(saved.revision(), disabled.revision());
        assert!(saved.device().is_some());
    }

    #[tokio::test]
    async fn successful_revoke_cas_clears_credentials() {
        let (_directory, store, profile) = configured();
        let enrolled = store.attach_device(profile.revision(), device()).unwrap();
        let plan = prepare(&store, Some(enrolled.revision()));
        let (client, task) = relay_server(200, r#"{"success":true}"#).await;
        finish(store.clone(), client, plan).await.unwrap();
        assert!(task
            .await
            .unwrap()
            .starts_with("POST /devices/revoke HTTP/1.1"));
        let saved = store.load().unwrap().unwrap();
        assert!(!saved.enabled());
        assert!(saved.device().is_none());
    }

    #[tokio::test]
    async fn later_profile_edit_prevents_cas_clear() {
        let (_directory, store, profile) = configured();
        let enrolled = store.attach_device(profile.revision(), device()).unwrap();
        let plan = prepare(&store, Some(enrolled.revision()));
        let disabled = plan.profile.as_ref().unwrap().clone();
        store.disable(disabled.revision()).unwrap();
        let (client, task) = relay_server(200, r#"{"success":true}"#).await;
        assert!(finish(store.clone(), client, plan).await.is_err());
        assert!(task
            .await
            .unwrap()
            .starts_with("POST /devices/revoke HTTP/1.1"));
        let saved = store.load().unwrap().unwrap();
        assert!(!saved.enabled());
        assert!(saved.device().is_some());
        assert_ne!(saved.revision(), disabled.revision());
    }

    #[tokio::test]
    async fn stale_or_load_error_plans_never_contact_the_relay() {
        let (_directory, store, profile) = configured();
        let enrolled = store.attach_device(profile.revision(), device()).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = RelayClient::build(
            reqwest::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap(),
            false,
        )
        .unwrap();
        let plan = prepare(&store, Some("stale-revision"));
        let error = finish(store.clone(), client.clone(), plan)
            .await
            .unwrap_err();
        assert!(error.contains("settings are not confirmed"));
        assert!(error.contains("no device snapshot was retained"));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("relay");
        std::fs::write(&path, b"not-a-directory").unwrap();
        let broken_store = Store::in_directory(path);
        let broken_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let broken_client = RelayClient::build(
            reqwest::Url::parse(&format!("http://{}", broken_listener.local_addr().unwrap()))
                .unwrap(),
            false,
        )
        .unwrap();
        let plan = prepare(&broken_store, None);
        let error = finish(broken_store, broken_client, plan).await.unwrap_err();
        assert!(error.contains("settings are not confirmed"));
        assert!(error.contains("no device snapshot was retained"));
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(50),
            broken_listener.accept()
        )
        .await
        .is_err());
        drop(enrolled);
    }
}
