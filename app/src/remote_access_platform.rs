// SPDX-License-Identifier: AGPL-3.0-only
//! Native listener and process inspection for Remote Access preflight.
//!
//! The probes in this module are deliberately fallible. A permission error,
//! malformed native response, or a race that prevents identity measurement is
//! not evidence that a port or process is absent.

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(windows)]
#[path = "remote_access_platform_windows.rs"]
mod windows;

#[cfg(windows)]
pub(crate) use windows::{process_identity, signal_process};

pub(crate) fn listener_pids_for_port(port: u16) -> Result<Vec<u32>, ()> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("/usr/sbin/lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-t"])
            .output()
            .map_err(|_| ())?;
        return parse_lsof_listener_pids(
            output.status.success(),
            output.status.code(),
            &output.stdout,
            &output.stderr,
        );
    }

    #[cfg(target_os = "linux")]
    {
        return linux::listener_pids_for_port(port);
    }

    #[cfg(windows)]
    {
        return windows::listener_pids_for_port(port);
    }

    #[allow(unreachable_code)]
    Err(())
}

/// Parse the strict lsof -t response used on macOS.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_lsof_listener_pids(
    success: bool,
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<Vec<u32>, ()> {
    if !success {
        return if code == Some(1) && stdout.is_empty() && stderr.is_empty() {
            Ok(Vec::new())
        } else {
            Err(())
        };
    }
    if !stderr.is_empty() {
        return Err(());
    }
    let text = std::str::from_utf8(stdout).map_err(|_| ())?;
    if text.trim().is_empty() {
        return Err(());
    }
    text.lines()
        .map(|line| {
            let pid = line.trim().parse::<u32>().map_err(|_| ())?;
            (pid > 0).then_some(pid).ok_or(())
        })
        .collect()
}

#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod linux {
    use std::collections::HashSet;
    use std::fs;
    use std::io;
    use std::path::Path;

    const TCP_LISTEN: u8 = 0x0a;

    pub(super) fn listener_pids_for_port(port: u16) -> Result<Vec<u32>, ()> {
        let mut inodes =
            parse_proc_net_tcp(&fs::read_to_string("/proc/net/tcp").map_err(|_| ())?, port)?;
        match fs::read_to_string("/proc/net/tcp6") {
            Ok(text) => inodes.extend(parse_proc_net_tcp(&text, port)?),
            // Linux omits this table when IPv6 support is disabled.
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(()),
        }
        if inodes.is_empty() {
            return Ok(Vec::new());
        }
        socket_owner_pids(&inodes)
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ProcSocket {
        inode: u64,
    }

    fn parse_proc_net_tcp(text: &str, port: u16) -> Result<Vec<u64>, ()> {
        let mut lines = text.lines();
        let header = lines.next().ok_or(())?;
        let header_fields: Vec<_> = header.split_whitespace().collect();
        if header_fields.first() != Some(&"sl")
            || header_fields.get(1) != Some(&"local_address")
            || !header_fields.contains(&"inode")
        {
            return Err(());
        }

        let mut sockets = Vec::new();
        for line in lines {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 10 {
                return Err(());
            }
            let slot = fields[0].strip_suffix(':').ok_or(())?;
            slot.parse::<u32>().map_err(|_| ())?;

            let (_, port_text) = fields[1].rsplit_once(':').ok_or(())?;
            let local_port = u16::from_str_radix(port_text, 16).map_err(|_| ())?;
            let state = u8::from_str_radix(fields[3], 16).map_err(|_| ())?;
            let inode = fields[9].parse::<u64>().map_err(|_| ())?;
            if state == TCP_LISTEN && local_port == port {
                if inode == 0 {
                    return Err(());
                }
                sockets.push(ProcSocket { inode });
            }
        }

        sockets.sort_unstable_by_key(|socket| socket.inode);
        sockets.dedup_by_key(|socket| socket.inode);
        Ok(sockets.into_iter().map(|socket| socket.inode).collect())
    }

    fn socket_owner_pids(inodes: &[u64]) -> Result<Vec<u32>, ()> {
        let wanted: HashSet<u64> = inodes.iter().copied().collect();
        let mut found_inodes = HashSet::new();
        let mut pids = HashSet::new();
        for entry in fs::read_dir("/proc").map_err(|_| ())? {
            let entry = entry.map_err(|_| ())?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Ok(pid) = name.parse::<u32>() else {
                continue;
            };
            if pid == 0 {
                continue;
            }
            scan_process_fds(
                &entry.path().join("fd"),
                pid,
                &wanted,
                &mut found_inodes,
                &mut pids,
            )?;
        }

        // A socket table row without a discoverable fd is an inspection race
        // or a permission boundary, not proof that the listener disappeared.
        if found_inodes.len() != wanted.len() {
            return Err(());
        }
        let mut pids: Vec<_> = pids.into_iter().collect();
        pids.sort_unstable();
        Ok(pids)
    }

    fn scan_process_fds(
        fd_dir: &Path,
        pid: u32,
        wanted: &HashSet<u64>,
        found_inodes: &mut HashSet<u64>,
        pids: &mut HashSet<u32>,
    ) -> Result<(), ()> {
        let entries = match fs::read_dir(fd_dir) {
            Ok(entries) => entries,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                ) =>
            {
                return Ok(());
            }
            Err(_) => return Err(()),
        };
        for entry in entries {
            let entry = entry.map_err(|_| ())?;
            let target = match fs::read_link(entry.path()) {
                Ok(target) => target,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                    ) =>
                {
                    continue
                }
                Err(_) => return Err(()),
            };
            let Some(target) = target.to_str() else {
                continue;
            };
            let Some(inode_text) = target
                .strip_prefix("socket:[")
                .and_then(|target| target.strip_suffix(']'))
            else {
                continue;
            };
            let inode = inode_text.parse::<u64>().map_err(|_| ())?;
            if wanted.contains(&inode) {
                found_inodes.insert(inode);
                pids.insert(pid);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::parse_proc_net_tcp;

        const HEADER: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode";

        #[test]
        fn parses_only_listeners_on_the_requested_port() {
            let text = format!(
                "{HEADER}\n  0: 0100007F:4690 00000000:0000 0A 00000000:00000000 00:00000000 00000000   1000       0 12345 1 0000000000000000 100 0 0 10 0\n  1: 0100007F:4691 00000000:0000 01 00000000:00000000 00:00000000 00000000   1000       0 12346 1 0000000000000000 100 0 0 10 0\n"
            );
            assert_eq!(parse_proc_net_tcp(&text, 0x4690), Ok(vec![12345]));
        }

        #[test]
        fn malformed_or_zero_inode_is_unknown() {
            assert!(parse_proc_net_tcp("bad header\n", 1).is_err());
            let zero = format!(
                "{HEADER}\n  0: 0100007F:4690 00000000:0000 0A 00000000:00000000 00:00000000 00000000   1000       0 0 1\n"
            );
            assert!(parse_proc_net_tcp(&zero, 0x4690).is_err());
        }

        #[test]
        fn unrelated_time_wait_without_inode_does_not_block_listener() {
            let text = format!("{HEADER}\n0: 0100007F:4690 00000000:0000 0A 0:0 00:0 0 1000 0 42\n1: 0100007F:4690 00000000:0000 06 0:0 00:0 0 1000 0 0\n");
            assert_eq!(parse_proc_net_tcp(&text, 0x4690), Ok(vec![42]));
        }

        #[test]
        fn ipv6_listener_and_duplicate_inodes_are_supported() {
            let row = "0: 00000000000000000000000001000000:4690 00000000000000000000000000000000:0000 0A 0:0 00:0 0 1000 0 42";
            assert_eq!(
                parse_proc_net_tcp(&format!("{HEADER}\n{row}\n{row}\n"), 0x4690),
                Ok(vec![42])
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn unresolved_socket_is_not_reported_as_absent() {
            assert_eq!(super::socket_owner_pids(&[u64::MAX]), Err(()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsof_errors_do_not_mean_absence() {
        assert_eq!(
            parse_lsof_listener_pids(false, Some(1), b"", b""),
            Ok(vec![])
        );
        assert!(parse_lsof_listener_pids(false, Some(1), b"", b"denied").is_err());
        assert!(parse_lsof_listener_pids(false, None, b"", b"").is_err());
        assert!(parse_lsof_listener_pids(true, Some(0), b"0\n", b"").is_err());
        assert!(parse_lsof_listener_pids(true, Some(0), b"", b"").is_err());
        assert_eq!(
            parse_lsof_listener_pids(true, Some(0), b"123\n456\n", b""),
            Ok(vec![123, 456])
        );
    }

    #[test]
    fn native_ipv4_listener_owner_is_measured() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(listener_pids_for_port(port)
            .unwrap()
            .contains(&std::process::id()));
    }

    #[test]
    fn native_ipv6_listener_owner_is_measured() {
        let listener = std::net::TcpListener::bind(("::1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(listener_pids_for_port(port)
            .unwrap()
            .contains(&std::process::id()));
    }
}
