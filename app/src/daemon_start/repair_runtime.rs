// SPDX-License-Identifier: AGPL-3.0-only
//! Resume only the positively measured owner of a verified cold repair.

use super::*;
use wenlan_types::repair_runtime::RepairRuntimeResumeApproval;

const RESUME_LIMIT: Duration = Duration::from_secs(45);
static RESUMING: AtomicBool = AtomicBool::new(false);
static PENDING: Mutex<Option<PendingResume>> = Mutex::new(None);

#[derive(Clone)]
struct PendingResume {
    approval: RepairRuntimeResumeApproval,
    instance_id: String,
    identity: SidecarIdentity,
    owner: ResumeOwner,
    uncertain_request: bool,
}

#[derive(Clone)]
enum ResumeOwner {
    Sidecar { generation: u64 },
    Service(crate::lifecycle::repair_service::RepairServiceOwner),
}

pub(super) fn ordinary_spawn_allowed() -> Result<(), String> {
    if crate::lifecycle::is_quitting() {
        Err("repair_runtime_app_quitting".into())
    } else if RESUMING.load(Ordering::Acquire) {
        Err("repair_runtime_resumption_in_progress".into())
    } else {
        Ok(())
    }
}

struct ResumeAdmission;

impl ResumeAdmission {
    fn acquire() -> Result<Self, String> {
        with_owner_decision(|| {
            ordinary_spawn_allowed()?;
            if launchd_install_pending() {
                return Err("repair_runtime_owner_changing".into());
            }
            RESUMING.store(true, Ordering::Release);
            Ok(Self)
        })
    }
}

impl Drop for ResumeAdmission {
    fn drop(&mut self) {
        with_owner_decision(|| RESUMING.store(false, Ordering::Release));
    }
}

fn ensure_not_quitting() -> Result<(), String> {
    if crate::lifecycle::is_quitting() {
        Err("repair_runtime_app_quitting".into())
    } else {
        Ok(())
    }
}

fn capture_owner(pid: u32) -> Result<(SidecarIdentity, ResumeOwner), String> {
    if let Some((generation, identity)) = SIDECAR
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .filter(|child| child.identity.pid.as_u32() == pid)
        .map(|child| (child.generation, child.identity))
    {
        if identity.presence() != ProcessPresence::Running {
            return Err("repair_runtime_owner_unmeasurable".into());
        }
        return Ok((identity, ResumeOwner::Sidecar { generation }));
    }
    let identity = SidecarIdentity::capture(pid);
    if identity.presence() != ProcessPresence::Running {
        return Err("repair_runtime_owner_unmeasurable".into());
    }
    let service = crate::lifecycle::repair_service::RepairServiceOwner::capture(pid)?;
    Ok((identity, ResumeOwner::Service(service)))
}

fn pending_for(
    approval: &RepairRuntimeResumeApproval,
    status: Option<&wenlan_types::repair_runtime::RepairRuntimeStatus>,
) -> Result<Option<PendingResume>, String> {
    let mut pending = PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // A positively gone owner cannot authorize this new cold instance. Drop
    // its record before approval matching, then measure the new owner afresh.
    // Offline retry keeps the old record; it is needed to restart that owner.
    if pending.as_ref().is_some_and(|previous| {
        status.is_some_and(|status| {
            status.repair_only
                && status.instance_id != previous.instance_id
                && previous.identity.presence() == ProcessPresence::Gone
        })
    }) {
        *pending = None;
    }
    match pending.as_ref() {
        Some(pending) if &pending.approval != approval => {
            Err("repair_runtime_other_resume_pending".into())
        }
        pending => Ok(pending.cloned()),
    }
}

fn record_resume_failure(
    saved: &mut Option<PendingResume>,
    error: &crate::api::RepairRuntimeRequestError,
) {
    let uncertain = saved.as_ref().is_some_and(|p| p.uncertain_request);
    if error.definitive_refusal(uncertain) {
        *saved = None;
    } else if let Some(saved) = saved.as_mut() {
        saved.uncertain_request = true;
    }
}

async fn attest_normal(
    client: &crate::api::WenlanClient,
    approval: &RepairRuntimeResumeApproval,
    instance_id: String,
) -> Result<(), String> {
    ensure_not_quitting()?;
    let status = client
        .resume_repair_runtime(approval.for_instance(instance_id.clone()))
        .await
        .map_err(|error| error.to_string())?;
    if status.instance_id != instance_id || status.repair_only || status.shutdown_requested {
        return Err("repair_runtime_not_normal".into());
    }
    client.activity().await?;
    ensure_not_quitting()
}

/// This is intentionally distinct from version healing: a same-version cold
/// daemon still needs a restart. No failure retries an apply or verification.
pub async fn resume_repair_runtime(
    app: &tauri::AppHandle,
    client: &crate::api::WenlanClient,
    approval: RepairRuntimeResumeApproval,
) -> Result<(), String> {
    // Share version-heal serialization, then reserve the owner decision across
    // awaits. Quit sets its latch under that same owner lock and wins every
    // later start decision. There is no synchronous guard across an await.
    let _version_heal = version_heal_lock().lock().await;
    let _admission = ResumeAdmission::acquire()?;
    let current = client.repair_runtime_status().await;
    // A normal successor authenticates the caller's current receipt directly;
    // an obsolete in-memory approval must not block that attestation.
    if let Ok(status) = &current {
        if !status.repair_only {
            attest_normal(client, &approval, status.instance_id.clone()).await?;
            *PENDING
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            return Ok(());
        }
    }
    let mut pending = pending_for(&approval, current.as_ref().ok())?;
    match current {
        Ok(status) => {
            ensure_not_quitting()?;
            if let Some(previous) = pending.as_ref() {
                if previous.instance_id != status.instance_id
                    || previous.identity.pid.as_u32() != status.pid
                {
                    return Err("repair_runtime_instance_changed".into());
                }
                if previous.identity.presence() != ProcessPresence::Running {
                    return Err("repair_runtime_owner_unmeasurable".into());
                }
            } else {
                if status.shutdown_requested {
                    return Err("repair_runtime_external_shutdown".into());
                }
                let (identity, owner) = with_owner_decision(|| capture_owner(status.pid))?;
                let measured = PendingResume {
                    approval: approval.clone(),
                    instance_id: status.instance_id.clone(),
                    identity,
                    owner,
                    uncertain_request: false,
                };
                *PENDING
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(measured.clone());
                pending = Some(measured);
            }
            if !status.shutdown_requested {
                // Keep measured ownership BEFORE the request: a lost response
                // cannot erase the identity needed for a safe retry.
                let response = client
                    .resume_repair_runtime(approval.for_instance(status.instance_id.clone()))
                    .await;
                let response = match response {
                    Ok(response) => response,
                    Err(error) => {
                        let mut saved = PENDING
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        record_resume_failure(&mut saved, &error);
                        return Err(error.to_string());
                    }
                };
                if let Err(error) = validate_shutdown_response(&status, &response) {
                    record_resume_failure(
                        &mut PENDING
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner),
                        &crate::api::RepairRuntimeRequestError::Uncertain(error.clone()),
                    );
                    return Err(error);
                }
            }
        }
        Err(error) if pending.is_none() => return Err(error),
        Err(_) => {} // Only a previously measured attempt may recover offline.
    }
    let pending = pending.ok_or("repair_runtime_owner_missing")?;
    let deadline = std::time::Instant::now() + RESUME_LIMIT;
    loop {
        ensure_not_quitting()?;
        match pending.identity.presence() {
            ProcessPresence::Gone => break,
            ProcessPresence::Running => {}
            ProcessPresence::Unknown => return Err("repair_runtime_owner_unmeasurable".into()),
        }
        if std::time::Instant::now() >= deadline {
            return Err("repair_runtime_stop_timeout".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // A service manager may already have started the successor. Authenticate
    // it before any start action; a responding foreign root is never stopped.
    if let Ok(status) = client.repair_runtime_status().await {
        if status.repair_only {
            return Err("repair_runtime_unexpected_recovery".into());
        }
        attest_normal(client, &approval, status.instance_id).await?;
        *PENDING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        return Ok(());
    }
    // Check the socket, not merely HTTP health: an unresponsive listener is
    // occupied and must not be treated as permission to spawn a rival.
    if listener_occupied(client).await? {
        return Err("repair_runtime_listener_occupied".into());
    }
    with_owner_decision(|| {
        ensure_not_quitting()?;
        if launchd_install_pending() {
            return Err("repair_runtime_owner_changing".into());
        }
        match &pending.owner {
            ResumeOwner::Sidecar { generation } => {
                forget_sidecar(*generation);
                if sidecar_alive() {
                    return Err("repair_runtime_owner_changed".into());
                }
                // An isolated sidecar must never consult or mutate the user's
                // LaunchAgents. Its exact child record is sufficient authority.
                if !crate::lifecycle::data_dir_env_overridden()
                    && crate::lifecycle::launchd_owns_server_daemon(
                        &crate::lifecycle::SystemLaunchctl,
                    ) != crate::lifecycle::LaunchdOwnership::DoesNot
                {
                    return Err("repair_runtime_owner_changed".into());
                }
                spawn_daemon_sidecar_unlocked(app)
            }
            ResumeOwner::Service(service) => service.start(),
        }
    })?;
    let deadline = std::time::Instant::now() + RESUME_LIMIT;
    loop {
        ensure_not_quitting()?;
        if let Ok(status) = client.repair_runtime_status().await {
            if status.repair_only || status.instance_id == pending.instance_id {
                return Err("repair_runtime_unexpected_recovery".into());
            }
            attest_normal(client, &approval, status.instance_id).await?;
            *PENDING
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err("repair_runtime_start_timeout".into());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn validate_shutdown_response(
    measured: &wenlan_types::repair_runtime::RepairRuntimeStatus,
    response: &wenlan_types::repair_runtime::RepairRuntimeStatus,
) -> Result<(), String> {
    if response.instance_id != measured.instance_id
        || response.pid != measured.pid
        || !response.repair_only
        || !response.shutdown_requested
    {
        return Err("repair_runtime_shutdown_unconfirmed".into());
    }
    Ok(())
}

async fn listener_occupied(client: &crate::api::WenlanClient) -> Result<bool, String> {
    let url = reqwest::Url::parse(client.base_url()).map_err(|_| "repair_runtime_invalid_host")?;
    let host = url.host_str().ok_or("repair_runtime_invalid_host")?;
    let port = url
        .port_or_known_default()
        .ok_or("repair_runtime_invalid_host")?;
    match tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect((host, port)),
    )
    .await
    {
        Ok(Ok(_)) => Ok(true),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => Ok(false),
        _ => Err("repair_runtime_listener_unmeasurable".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wenlan_types::repair_runtime::RepairRuntimeStatus;

    fn approval(digit: char) -> RepairRuntimeResumeApproval {
        use wenlan_types::repair::{ApplyRepairRequest, RepairDigest};
        let id = format!("repair_550e8400-e29b-41d4-a716-44665544000{digit}");
        let digest = RepairDigest::parse(&"ab".repeat(32)).unwrap();
        RepairRuntimeResumeApproval {
            apply: ApplyRepairRequest::try_new(
                id.clone(),
                digest.clone(),
                format!("apply repair {id} {}", digest.as_str()),
            )
            .unwrap(),
            verification_receipt_digest: RepairDigest::parse(&"cd".repeat(32)).unwrap(),
        }
    }

    fn pending_fixture() -> PendingResume {
        PendingResume {
            approval: approval('1'),
            instance_id: "old".into(),
            identity: SidecarIdentity::capture(std::process::id()),
            owner: ResumeOwner::Sidecar { generation: 17 },
            uncertain_request: false,
        }
    }

    #[test]
    #[serial_test::serial]
    fn repair_runtime_refusal_releases_old_approval_for_next_repair() {
        for code in [
            "repair_background_writer_busy",
            "repair_runtime_receipt_mismatch",
            "repair_runtime_pending_repairs",
        ] {
            let mut saved = PENDING.lock().unwrap();
            *saved = Some(pending_fixture());
            record_resume_failure(
                &mut saved,
                &crate::api::RepairRuntimeRequestError::Http {
                    status: 409,
                    body: format!(r#"{{"error":"{code}"}}"#),
                },
            );
            assert!(saved.is_none());
            drop(saved);
            assert!(pending_for(&approval('2'), None).unwrap().is_none());
        }
    }

    #[test]
    #[serial_test::serial]
    fn repair_runtime_lost_response_retains_exact_owner_for_offline_retry() {
        let mut saved = PENDING.lock().unwrap();
        *saved = Some(pending_fixture());
        record_resume_failure(
            &mut saved,
            &crate::api::RepairRuntimeRequestError::Uncertain("response lost".into()),
        );
        // A still-running earlier request can own the provisional seal.
        record_resume_failure(
            &mut saved,
            &crate::api::RepairRuntimeRequestError::Http {
                status: 409,
                body: r#"{"error":"repair_background_writer_busy"}"#.into(),
            },
        );
        assert!(saved.as_ref().unwrap().uncertain_request);
        drop(saved);
        let retry = pending_for(&approval('1'), None).unwrap().unwrap();
        assert_eq!(retry.instance_id, "old");
        assert_eq!(retry.identity.pid.as_u32(), std::process::id());
        assert!(matches!(
            retry.owner,
            ResumeOwner::Sidecar { generation: 17 }
        ));
        assert!(pending_for(&approval('2'), None).is_err());
        *PENDING.lock().unwrap() = None;
    }

    #[test]
    #[serial_test::serial]
    fn repair_runtime_new_cold_owner_replaces_only_a_positively_gone_record() {
        let current = RepairRuntimeStatus {
            instance_id: "new".into(),
            pid: std::process::id(),
            repair_only: true,
            shutdown_requested: false,
        };
        *PENDING.lock().unwrap() = Some(pending_fixture());
        assert!(pending_for(&approval('2'), Some(&current)).is_err());
        PENDING
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .identity
            .started_at = None;
        assert!(pending_for(&approval('2'), Some(&current)).is_err());
        // This PID is alive, but a different start time proves the old owner ended.
        PENDING
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .identity
            .started_at = Some(u64::MAX);
        assert!(pending_for(&approval('2'), Some(&current))
            .unwrap()
            .is_none());
    }

    #[test]
    #[serial_test::serial]
    fn repair_runtime_admission_excludes_owner_changes_until_drop() {
        let admission = ResumeAdmission::acquire().unwrap();
        assert!(ResumeAdmission::acquire().is_err());
        assert!(LaunchdInstallPending::try_begin_user_change().is_err());
        assert!(ordinary_spawn_allowed().is_err());
        drop(admission);
        assert!(ordinary_spawn_allowed().is_ok());
        let installation = LaunchdInstallPending::try_begin_user_change().unwrap();
        assert!(ResumeAdmission::acquire().is_err());
        drop(installation);
        assert!(ResumeAdmission::acquire().is_ok());
    }

    #[test]
    fn repair_runtime_requires_shutdown_of_the_measured_cold_instance() {
        let measured = RepairRuntimeStatus {
            instance_id: "original".into(),
            pid: 42,
            repair_only: true,
            shutdown_requested: false,
        };
        let accepted = RepairRuntimeStatus {
            shutdown_requested: true,
            ..measured.clone()
        };
        assert!(validate_shutdown_response(&measured, &accepted).is_ok());
        for response in [
            measured.clone(),
            RepairRuntimeStatus {
                instance_id: "successor".into(),
                ..accepted.clone()
            },
            RepairRuntimeStatus {
                pid: 43,
                ..accepted.clone()
            },
            RepairRuntimeStatus {
                repair_only: false,
                ..accepted.clone()
            },
        ] {
            assert!(validate_shutdown_response(&measured, &response).is_err());
        }
    }
}
