// SPDX-License-Identifier: AGPL-3.0-only
//! Bounded verification for processes owned by the desktop app.

use std::time::Duration;

use tauri_plugin_shell::process::CommandChild;

const CONFIRM_LIMIT: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// A process the app asked to stop but has not yet proved exited.
///
/// The start time pins the record to the process the app spawned. `None` is a
/// failed identity measurement, not permission to signal a pid occupant.
#[derive(Clone, Debug)]
pub(crate) struct PendingProcess {
    pid: sysinfo::Pid,
    started_at: Option<u64>,
    kill_error: Option<bool>,
}

/// Capture the child identity before spending its one-shot child handle.
pub(crate) fn request_stop(child: CommandChild) -> PendingProcess {
    let pid = child.pid();
    let mut pending = PendingProcess::capture(pid);
    pending.kill_error = Some(child.kill().is_err());
    pending
}

/// Request all stops in one synchronous batch. The caller can place this
/// function in one `spawn_blocking` task so identity reads and handle kills do
/// not run on the async executor.
pub(crate) fn request_stops(
    children: Vec<CommandChild>,
    mut pending: Vec<PendingProcess>,
) -> Vec<PendingProcess> {
    pending.extend(children.into_iter().map(request_stop));
    pending
}

/// Confirm exits for at most two seconds, returning every record that could
/// not be proven gone. This function never sends a second signal.
pub(crate) async fn confirm_exits(pending: Vec<PendingProcess>) -> Vec<PendingProcess> {
    if pending.is_empty() {
        return pending;
    }

    let deadline = tokio::time::Instant::now() + CONFIRM_LIMIT;
    let mut unconfirmed = pending;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return unconfirmed;
        }

        // Keep the original batch until the join succeeds. A panic or timeout
        // in the blocking task must not make the caller lose ownership state.
        let batch = unconfirmed.clone();
        let observation = match tokio::time::timeout(
            remaining,
            tokio::task::spawn_blocking(move || refresh_processes(&batch)),
        )
        .await
        {
            Ok(Ok(observation)) => observation,
            Ok(Err(_)) | Err(_) => return unconfirmed,
        };

        unconfirmed = retain_unconfirmed(unconfirmed, &observation);
        if unconfirmed.is_empty() {
            return unconfirmed;
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return unconfirmed;
        }
        tokio::time::sleep(POLL_INTERVAL.min(remaining)).await;
    }
}

impl PendingProcess {
    /// Preserve a child pid when the task that owned the handle did not
    /// return. The caller must not signal this record because its identity was
    /// never measured.
    pub(crate) fn unmeasured(raw_pid: u32) -> Self {
        Self {
            pid: sysinfo::Pid::from_u32(raw_pid),
            started_at: None,
            kill_error: None,
        }
    }

    fn capture(raw_pid: u32) -> Self {
        let pid = sysinfo::Pid::from_u32(raw_pid);
        let mut system = sysinfo::System::new();
        let pids = [pid];
        system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&pids), true);
        let started_at = system
            .process(pid)
            .and_then(|process| nonzero_start_time(process.start_time()));
        Self {
            pid,
            started_at,
            kill_error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProcessObservation {
    Absent {
        independent: IndependentProcessObservation,
    },
    Present {
        started_at: Option<u64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndependentProcessObservation {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Debug)]
struct RefreshObservation {
    app_observed: bool,
    targets: Vec<(sysinfo::Pid, ProcessObservation)>,
}

fn refresh_processes(pending: &[PendingProcess]) -> RefreshObservation {
    let app_pid = match sysinfo::get_current_pid() {
        Ok(pid) => pid,
        Err(_) => {
            return RefreshObservation {
                app_observed: false,
                targets: Vec::new(),
            }
        }
    };

    let mut pids = Vec::with_capacity(pending.len() + 1);
    pids.push(app_pid);
    pids.extend(pending.iter().map(|process| process.pid));

    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&pids), true);

    let app_observed = system.process(app_pid).is_some();
    let targets = pending
        .iter()
        .map(|process| {
            let observation = match system.process(process.pid) {
                Some(current) => ProcessObservation::Present {
                    started_at: nonzero_start_time(current.start_time()),
                },
                None => ProcessObservation::Absent {
                    independent: independent_process_observation(process.pid),
                },
            };
            (process.pid, observation)
        })
        .collect();

    RefreshObservation {
        app_observed,
        targets,
    }
}

fn retain_unconfirmed(
    pending: Vec<PendingProcess>,
    observation: &RefreshObservation,
) -> Vec<PendingProcess> {
    pending
        .into_iter()
        .filter(|process| !observation.proves_exit(process))
        .collect()
}

impl RefreshObservation {
    fn proves_exit(&self, process: &PendingProcess) -> bool {
        match self
            .targets
            .iter()
            .find(|(pid, _)| *pid == process.pid)
            .map(|(_, observation)| observation)
        {
            Some(ProcessObservation::Absent {
                independent: IndependentProcessObservation::Absent,
            }) => self.app_observed,
            Some(ProcessObservation::Present {
                started_at: Some(current),
            }) => process
                .started_at
                .is_some_and(|recorded| recorded != *current),
            // A present process whose identity cannot be read is unknown. A
            // missing capture is likewise unknown while any occupant exists.
            Some(ProcessObservation::Absent {
                independent:
                    IndependentProcessObservation::Present | IndependentProcessObservation::Unknown,
            })
            | Some(ProcessObservation::Present { started_at: None })
            | None => false,
        }
    }
}

fn nonzero_start_time(started_at: u64) -> Option<u64> {
    (started_at != 0).then_some(started_at)
}

#[cfg(unix)]
fn independent_process_observation(pid: sysinfo::Pid) -> IndependentProcessObservation {
    let pid = pid.as_u32();
    let output = match std::process::Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "pid="])
        .output()
    {
        Ok(output) => output,
        Err(_) => return IndependentProcessObservation::Unknown,
    };

    classify_ps_result(
        output.status.success(),
        output.status.code(),
        &output.stdout,
        &output.stderr,
        pid,
    )
}

#[cfg(unix)]
fn classify_ps_result(
    success: bool,
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    expected_pid: u32,
) -> IndependentProcessObservation {
    if success {
        let reports_expected_pid = std::str::from_utf8(stdout)
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
            == Some(expected_pid);
        return if reports_expected_pid {
            IndependentProcessObservation::Present
        } else {
            IndependentProcessObservation::Unknown
        };
    }

    if code == Some(1) && stdout.is_empty() && stderr.is_empty() {
        IndependentProcessObservation::Absent
    } else {
        IndependentProcessObservation::Unknown
    }
}

#[cfg(windows)]
fn independent_process_observation(pid: sysinfo::Pid) -> IndependentProcessObservation {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    let raw_pid = pid.as_u32();
    if raw_pid == 0 {
        return IndependentProcessObservation::Unknown;
    }

    // SYNCHRONIZE is read-only here: opening a process handle and observing
    // its signaled state never sends a termination request.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, raw_pid) };
    if handle.is_null() {
        return if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
            IndependentProcessObservation::Absent
        } else {
            IndependentProcessObservation::Unknown
        };
    }

    let state = unsafe { WaitForSingleObject(handle, 0) };
    unsafe {
        CloseHandle(handle);
    }

    match state {
        WAIT_OBJECT_0 => IndependentProcessObservation::Absent,
        WAIT_TIMEOUT => IndependentProcessObservation::Present,
        _ => IndependentProcessObservation::Unknown,
    }
}

#[cfg(not(any(unix, windows)))]
fn independent_process_observation(_pid: sysinfo::Pid) -> IndependentProcessObservation {
    IndependentProcessObservation::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(pid: u32, started_at: Option<u64>, kill_error: Option<bool>) -> PendingProcess {
        PendingProcess {
            pid: sysinfo::Pid::from_u32(pid),
            started_at,
            kill_error,
        }
    }

    fn observation(
        app_observed: bool,
        pid: u32,
        process: ProcessObservation,
    ) -> RefreshObservation {
        RefreshObservation {
            app_observed,
            targets: vec![(sysinfo::Pid::from_u32(pid), process)],
        }
    }

    fn apply_observations(
        mut pending: Vec<PendingProcess>,
        observations: impl IntoIterator<Item = RefreshObservation>,
    ) -> Vec<PendingProcess> {
        for observation in observations {
            pending = retain_unconfirmed(pending, &observation);
            if pending.is_empty() {
                break;
            }
        }
        pending
    }

    #[test]
    fn delayed_exit_is_confirmed_on_a_fresh_follow_up_observation() {
        let pending = pending(41, Some(100), Some(false));
        let survivors = apply_observations(
            vec![pending],
            [
                observation(
                    true,
                    41,
                    ProcessObservation::Present {
                        started_at: Some(100),
                    },
                ),
                observation(
                    true,
                    41,
                    ProcessObservation::Absent {
                        independent: IndependentProcessObservation::Absent,
                    },
                ),
            ],
        );
        assert!(survivors.is_empty());
    }

    #[test]
    fn still_running_is_returned_for_the_caller_to_retry_later() {
        let survivors = apply_observations(
            vec![pending(42, Some(200), Some(false))],
            [observation(
                true,
                42,
                ProcessObservation::Present {
                    started_at: Some(200),
                },
            )],
        );
        assert_eq!(survivors.len(), 1);
    }

    #[test]
    fn unknown_refresh_or_unreadable_identity_never_counts_as_gone() {
        let unknown_app = apply_observations(
            vec![pending(43, Some(300), Some(false))],
            [observation(
                false,
                43,
                ProcessObservation::Absent {
                    independent: IndependentProcessObservation::Absent,
                },
            )],
        );
        assert_eq!(unknown_app.len(), 1);

        let untrusted_absence = apply_observations(
            vec![pending(48, Some(450), Some(false))],
            [observation(
                true,
                48,
                ProcessObservation::Absent {
                    independent: IndependentProcessObservation::Unknown,
                },
            )],
        );
        assert_eq!(untrusted_absence.len(), 1);

        let unknown_target = apply_observations(
            vec![pending(44, Some(400), Some(false))],
            [observation(
                true,
                44,
                ProcessObservation::Present { started_at: None },
            )],
        );
        assert_eq!(unknown_target.len(), 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn owned_running_child_is_retained_until_a_retry_proves_exit() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("5")
            .spawn()
            .expect("spawn harmless owned fixture");
        let pending = PendingProcess::capture(child.id());
        let remaining = confirm_exits(vec![pending]).await;
        // Cleanup precedes assertions so even a failed probe cannot leak a child.
        let killed = child.kill();
        let reaped = child.wait();
        assert!(killed.is_ok());
        assert!(reaped.is_ok());
        assert_eq!(remaining.len(), 1);
        assert!(confirm_exits(remaining).await.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn ps_absence_requires_exact_empty_exit_one_result() {
        assert_eq!(
            classify_ps_result(false, Some(1), b"", b"", 49),
            IndependentProcessObservation::Absent
        );
        assert_eq!(
            classify_ps_result(true, Some(0), b" 49\n", b"", 49),
            IndependentProcessObservation::Present
        );
        assert_eq!(
            classify_ps_result(true, Some(0), b"not-a-pid\n", b"", 49),
            IndependentProcessObservation::Unknown
        );
        assert_eq!(
            classify_ps_result(false, Some(1), b"", b"permission denied", 49),
            IndependentProcessObservation::Unknown
        );
    }

    #[test]
    fn unmeasured_pid_stays_pending_until_healthy_absence_is_observed() {
        let unmeasured = PendingProcess::unmeasured(47);
        assert_eq!(unmeasured.started_at, None);
        let replacement = apply_observations(
            vec![unmeasured.clone()],
            [observation(
                true,
                47,
                ProcessObservation::Present {
                    started_at: Some(701),
                },
            )],
        );
        assert_eq!(replacement.len(), 1);

        let gone = apply_observations(
            vec![unmeasured],
            [observation(
                true,
                47,
                ProcessObservation::Absent {
                    independent: IndependentProcessObservation::Absent,
                },
            )],
        );
        assert!(gone.is_empty());
    }

    #[test]
    fn identity_change_proves_original_exit_without_matching_the_replacement() {
        let survivors = apply_observations(
            vec![pending(45, Some(500), Some(false))],
            [observation(
                true,
                45,
                ProcessObservation::Present {
                    started_at: Some(501),
                },
            )],
        );
        assert!(survivors.is_empty());
    }

    #[test]
    fn kill_failure_does_not_hide_a_later_proven_exit() {
        let failed = pending(46, Some(600), Some(true));
        assert_eq!(failed.kill_error, Some(true));
        let survivors = apply_observations(
            vec![failed],
            [
                observation(
                    true,
                    46,
                    ProcessObservation::Present {
                        started_at: Some(600),
                    },
                ),
                observation(
                    true,
                    46,
                    ProcessObservation::Absent {
                        independent: IndependentProcessObservation::Absent,
                    },
                ),
            ],
        );
        assert!(survivors.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn owned_short_lived_child_absence_is_confirmed_by_ps() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("0.1")
            .spawn()
            .expect("spawn harmless short-lived fixture");
        let pending = PendingProcess::capture(child.id());
        child.wait().expect("wait for owned fixture");

        let observation = refresh_processes(std::slice::from_ref(&pending));
        assert!(observation.app_observed);
        assert!(matches!(
            observation.targets.first(),
            Some((
                _,
                ProcessObservation::Absent {
                    independent: IndependentProcessObservation::Absent
                }
            ))
        ));
    }
}
