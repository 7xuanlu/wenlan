// SPDX-License-Identifier: AGPL-3.0-only
//! Measure a loaded job, then start that unchanged job without a kill/reinstall.

#[cfg(any(target_os = "macos", test))]
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub(crate) struct RepairServiceOwner {
    #[cfg(target_os = "macos")]
    target: String,
    #[cfg(target_os = "macos")]
    plist: std::path::PathBuf,
    #[cfg(target_os = "macos")]
    plist_digest: [u8; 32],
    #[cfg(target_os = "macos")]
    loaded_digest: [u8; 32],
}

#[cfg(any(target_os = "macos", test))]
struct LoadedJob {
    pid: Option<u32>,
    path: String,
    program: String,
    data_root: String,
    digest: [u8; 32],
}

impl RepairServiceOwner {
    pub(crate) fn capture(pid: u32) -> Result<Self, String> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = pid;
            Err("repair_runtime_service_owner_unsupported".into())
        }
        #[cfg(target_os = "macos")]
        {
            if super::data_dir_env_overridden() {
                return Err("repair_runtime_foreign_service".into());
            }
            let plist =
                super::server_plist_path().map_err(|_| "repair_runtime_service_unavailable")?;
            let bytes = std::fs::read(&plist).map_err(|_| "repair_runtime_service_unavailable")?;
            let contents =
                std::str::from_utf8(&bytes).map_err(|_| "repair_runtime_service_unavailable")?;
            if !super::server_plist_has_selected_data_dir(contents) {
                return Err("repair_runtime_service_root_mismatch".into());
            }
            let uid = std::process::Command::new("/usr/bin/id")
                .arg("-u")
                .output()
                .map_err(|_| "repair_runtime_service_unavailable")?;
            let uid = if uid.status.success() {
                std::str::from_utf8(&uid.stdout)
                    .ok()
                    .and_then(|v| v.trim().parse::<u32>().ok())
            } else {
                None
            }
            .ok_or("repair_runtime_service_unavailable")?;
            let target = format!("gui/{uid}/{}", super::SERVER_PLIST_LABEL);
            let loaded = read_job(&super::SystemLaunchctl, &target)?;
            if loaded.pid != Some(pid) {
                return Err("repair_runtime_service_pid_mismatch".into());
            }
            let configured_program =
                super::plist_first_program(contents).ok_or("repair_runtime_service_unavailable")?;
            if canonical(&loaded.path)? != canonical(&plist)?
                || canonical(&loaded.program)? != canonical(&configured_program)?
            {
                return Err("repair_runtime_service_program_mismatch".into());
            }
            let (_, root) = crate::identity_paths::sidecar_data_dir_env();
            if canonical(&loaded.data_root)? != canonical(&root)? {
                return Err("repair_runtime_service_root_mismatch".into());
            }
            let process_id = sysinfo::Pid::from_u32(pid);
            let mut system = sysinfo::System::new();
            system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[process_id]), true);
            let executable = system
                .process(process_id)
                .and_then(|p| p.exe())
                .ok_or("repair_runtime_owner_unmeasurable")?;
            if canonical(executable)? != canonical(&loaded.program)? {
                return Err("repair_runtime_service_program_mismatch".into());
            }
            Ok(Self {
                target,
                plist,
                plist_digest: Sha256::digest(bytes).into(),
                loaded_digest: loaded.digest,
            })
        }
    }

    /// No `-k`: a newer instance is never killed by a delayed retry. No
    /// bootstrap, bootout, registration rewrite, or default-daemon fallback.
    pub(crate) fn start(&self) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        {
            Err("repair_runtime_service_owner_unsupported".into())
        }
        #[cfg(target_os = "macos")]
        {
            let bytes =
                std::fs::read(&self.plist).map_err(|_| "repair_runtime_service_unavailable")?;
            let digest: [u8; 32] = Sha256::digest(bytes).into();
            if digest != self.plist_digest
                || read_job(&super::SystemLaunchctl, &self.target)?.digest != self.loaded_digest
            {
                return Err("repair_runtime_service_configuration_changed".into());
            }
            start_job(&super::SystemLaunchctl, &self.target)
        }
    }
}

#[cfg(target_os = "macos")]
fn canonical(path: impl AsRef<std::path::Path>) -> Result<std::path::PathBuf, String> {
    std::fs::canonicalize(path).map_err(|_| "repair_runtime_service_path_unmeasurable".into())
}

#[cfg(any(target_os = "macos", test))]
fn start_job(launchctl: &dyn super::LaunchctlExec, target: &str) -> Result<(), String> {
    let output = launchctl
        .run(&["kickstart", target])
        .map_err(|_| "repair_runtime_service_start_failed")?;
    if output.status.success() {
        Ok(())
    } else {
        Err("repair_runtime_service_start_failed".into())
    }
}

#[cfg(target_os = "macos")]
fn read_job(launchctl: &dyn super::LaunchctlExec, target: &str) -> Result<LoadedJob, String> {
    let output = launchctl
        .run(&["print", target])
        .map_err(|_| "repair_runtime_service_unavailable")?;
    if !output.status.success() || output.stdout.len() > 1024 * 1024 {
        return Err("repair_runtime_service_unavailable".into());
    }
    let text =
        std::str::from_utf8(&output.stdout).map_err(|_| "repair_runtime_service_unavailable")?;
    let (root_key, _) = crate::identity_paths::sidecar_data_dir_env();
    parse_job(text, target, root_key).ok_or_else(|| "repair_runtime_service_unmeasurable".into())
}

/// `launchctl print` is not JSON. Only accept its complete, observed top-level
/// scalar/block form. PID and running counters are not launch provenance; the
/// program, arguments and all three environment blocks are. Unknown or partial
/// shapes fail closed rather than guessing a PID or silently losing arguments.
#[cfg(any(target_os = "macos", test))]
fn parse_job(text: &str, target: &str, root_key: &str) -> Option<LoadedJob> {
    use std::collections::BTreeMap;
    if !text.ends_with("}\n") {
        return None;
    }
    let mut lines = text.lines();
    if lines.next()? != format!("{target} = {{") {
        return None;
    }
    let mut fields = BTreeMap::new();
    let mut closed = false;
    while let Some(line) = lines.next() {
        if line == "}" {
            if lines.any(|line| !line.is_empty()) {
                return None;
            }
            closed = true;
            break;
        }
        if line.is_empty() {
            continue;
        }
        let root = line.strip_prefix('\t')?;
        if root.starts_with('\t') {
            return None;
        }
        let Some((key, value)) = root.split_once(" = ") else {
            continue;
        };
        let value = if value == "{" {
            let mut block = Vec::new();
            loop {
                let line = lines.next()?;
                if line == "\t}" {
                    break;
                }
                if !line.starts_with("\t\t") {
                    return None;
                }
                block.push(line.to_string());
            }
            block.join("\n")
        } else {
            value.to_string()
        };
        if fields.insert(key.to_string(), value).is_some() {
            return None;
        }
    }
    if !closed {
        return None;
    }
    let pid = match fields.get("pid") {
        Some(pid) => Some(pid.parse::<u32>().ok().filter(|pid| *pid > 0)?),
        None => None,
    };
    let path = fields.get("path")?.trim_matches('"').to_string();
    let program = fields.get("program")?.trim_matches('"').to_string();
    let environment = fields.get("environment")?;
    let roots: Vec<_> = environment
        .lines()
        .filter_map(|line| line.trim().split_once(" => "))
        .filter(|(key, _)| *key == root_key)
        .collect();
    if roots.len() != 1 {
        return None;
    }
    let data_root = roots[0].1.trim_matches('"').to_string();
    let mut hash = Sha256::new();
    for key in [
        "path",
        "program",
        "arguments",
        "inherited environment",
        "default environment",
        "environment",
    ] {
        let value = fields.get(key);
        if ["path", "program", "arguments", "environment"].contains(&key) && value.is_none() {
            return None;
        }
        hash.update(key.as_bytes());
        hash.update([0]);
        if let Some(value) = value {
            hash.update(value.as_bytes());
        }
        hash.update([0]);
    }
    Some(LoadedJob {
        pid,
        path,
        program,
        data_root,
        digest: hash.finalize().into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &str = "gui/501/com.wenlan.server";
    const ROOT_KEY: &str = "WENLAN_DATA_DIR";

    fn fixture() -> String {
        format!("{TARGET} = {{\n\tpath = /scratch/server.plist\n\tprogram = /scratch/wenlan-server\n\targuments = {{\n\t\t/scratch/wenlan-server\n\t}}\n\tenvironment = {{\n\t\tWENLAN_DATA_DIR => /scratch/data\n\t}}\n\tpid = 42\n}}\n")
    }

    #[test]
    fn repair_service_provenance_survives_exit_but_changes_with_launch_configuration() {
        let text = fixture();
        let running = parse_job(&text, TARGET, ROOT_KEY).unwrap();
        let stopped = parse_job(&text.replace("\tpid = 42\n", ""), TARGET, ROOT_KEY).unwrap();
        assert_eq!(running.pid, Some(42));
        assert_eq!(running.path, "/scratch/server.plist");
        assert_eq!(running.program, "/scratch/wenlan-server");
        assert_eq!(stopped.pid, None);
        assert_eq!(running.digest, stopped.digest);
        for (from, to) in [
            ("/scratch/server.plist", "/other/server.plist"),
            ("/scratch/wenlan-server", "/other/wenlan-server"),
            ("/scratch/data", "/other/data"),
        ] {
            assert_ne!(
                running.digest,
                parse_job(&text.replace(from, to), TARGET, ROOT_KEY)
                    .unwrap()
                    .digest
            );
        }
        assert_eq!(running.data_root, "/scratch/data");
    }

    #[test]
    fn repair_service_rejects_truncated_ambiguous_or_foreign_metadata() {
        let text = fixture();
        for end in 0..text.len() {
            assert!(
                parse_job(&text[..end], TARGET, ROOT_KEY).is_none(),
                "accepted prefix {end}"
            );
        }
        for malformed in [
            text.replace("\tpid = 42", "\tpid = 0"),
            text.replace("\tpid = 42", "\tpid = unknown"),
            text.replace("\tpid = 42", "\tpid = 42\n\tpid = 43"),
            text.replace(
                "WENLAN_DATA_DIR => /scratch/data",
                "WENLAN_DATA_DIR => /scratch/data\n\t\tWENLAN_DATA_DIR => /other",
            ),
            text.replace("WENLAN_DATA_DIR", "UNRELATED_ROOT"),
            text.replace("gui/501", "gui/502"),
            format!("{text}unparsed trailing data\n"),
            text.replace("\tpid = 42\n}\n", ""),
        ] {
            assert!(parse_job(&malformed, TARGET, ROOT_KEY).is_none());
        }
    }

    #[test]
    fn repair_service_start_never_kills_or_reinstalls_job() {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        #[cfg(windows)]
        use std::os::windows::process::ExitStatusExt;
        struct Launchctl {
            calls: std::sync::Mutex<Vec<Vec<String>>>,
            success: bool,
        }
        impl crate::lifecycle::LaunchctlExec for Launchctl {
            fn run(&self, args: &[&str]) -> std::io::Result<std::process::Output> {
                self.calls
                    .lock()
                    .unwrap()
                    .push(args.iter().map(|arg| arg.to_string()).collect());
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(if self.success { 0 } else { 256 }),
                    stdout: vec![],
                    stderr: vec![],
                })
            }
        }
        for success in [true, false] {
            let launchctl = Launchctl {
                calls: Default::default(),
                success,
            };
            assert_eq!(start_job(&launchctl, TARGET).is_ok(), success);
            assert_eq!(
                *launchctl.calls.lock().unwrap(),
                vec![vec!["kickstart".to_string(), TARGET.to_string()]]
            );
        }
    }
}
