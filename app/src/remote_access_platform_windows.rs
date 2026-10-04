// SPDX-License-Identifier: AGPL-3.0-only

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, FILETIME, HANDLE, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, TerminateProcess,
    WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};

struct ProcessHandle(HANDLE);

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

impl ProcessHandle {
    fn open(pid: u32, terminate: bool) -> Result<Option<Self>, ()> {
        if pid == 0 {
            return Err(());
        }
        let rights = PROCESS_QUERY_LIMITED_INFORMATION
            | PROCESS_SYNCHRONIZE
            | if terminate { PROCESS_TERMINATE } else { 0 };
        let handle = unsafe { OpenProcess(rights, 0, pid) };
        if handle.is_null() {
            return if unsafe { GetLastError() } == ERROR_INVALID_PARAMETER {
                Ok(None)
            } else {
                Err(())
            };
        }
        Ok(Some(Self(handle)))
    }

    fn running(&self) -> Result<bool, ()> {
        match unsafe { WaitForSingleObject(self.0, 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            _ => Err(()),
        }
    }

    fn started(&self) -> Result<String, ()> {
        let mut times = [FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        }; 4];
        let result = unsafe {
            GetProcessTimes(
                self.0,
                &mut times[0],
                &mut times[1],
                &mut times[2],
                &mut times[3],
            )
        };
        if result == 0 {
            return Err(());
        }
        let ticks = (u64::from(times[0].dwHighDateTime) << 32) | u64::from(times[0].dwLowDateTime);
        if ticks == 0 {
            return Err(());
        }
        Ok(format!("win32:{ticks}"))
    }

    fn executable(&self) -> Result<String, ()> {
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(self.0, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
            return Err(());
        }
        let name = String::from_utf16(buffer.get(..length as usize).ok_or(())?).map_err(|_| ())?;
        if name.is_empty() {
            return Err(());
        }
        Ok(name)
    }
}

pub(crate) fn process_identity(pid: u32) -> Result<Option<(String, Vec<String>)>, ()> {
    let Some(handle) = ProcessHandle::open(pid, false)? else {
        return Ok(None);
    };
    if !handle.running()? {
        return Ok(None);
    }
    let started = handle.started()?;
    let executable = handle.executable()?;
    let process_id = sysinfo::Pid::from_u32(pid);
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[process_id]),
        true,
        sysinfo::ProcessRefreshKind::nothing().with_cmd(sysinfo::UpdateKind::Always),
    );
    let process = system.process(process_id).ok_or(())?;
    let mut args: Vec<String> = process
        .cmd()
        .iter()
        .map(|arg| arg.to_str().map(str::to_owned).ok_or(()))
        .collect::<Result<_, _>>()?;
    if args.is_empty() {
        return Err(());
    }
    // Use the kernel image path, not the caller-controlled argv[0]. Keeping
    // the handle open pins this process object throughout command inspection.
    args[0] = executable;
    if !handle.running()? {
        return Ok(None);
    }
    Ok(Some((started, args)))
}

pub(crate) fn signal_process(pid: u32, expected_identity: &str) -> bool {
    if pid == std::process::id() {
        return false;
    }
    let Some((expected_start, _)) = expected_identity.split_once('\n') else {
        return false;
    };
    let Ok(Some(handle)) = ProcessHandle::open(pid, true) else {
        return false;
    };
    if handle.started().as_deref() != Ok(expected_start) || handle.running() != Ok(true) {
        return false;
    }
    // Windows has no POSIX TERM for this console sidecar. Terminate only the
    // handle whose creation time still matches the approved orphan identity.
    unsafe { TerminateProcess(handle.0, 1) != 0 }
}

pub(super) fn listener_pids_for_port(port: u16) -> Result<Vec<u32>, ()> {
    let mut pids = table_pids(
        AF_INET.into(),
        port,
        std::mem::offset_of!(MIB_TCPTABLE_OWNER_PID, table),
        std::mem::size_of::<MIB_TCPROW_OWNER_PID>(),
        std::mem::offset_of!(MIB_TCPROW_OWNER_PID, dwLocalPort),
        std::mem::offset_of!(MIB_TCPROW_OWNER_PID, dwOwningPid),
    )?;
    pids.extend(table_pids(
        AF_INET6.into(),
        port,
        std::mem::offset_of!(MIB_TCP6TABLE_OWNER_PID, table),
        std::mem::size_of::<MIB_TCP6ROW_OWNER_PID>(),
        std::mem::offset_of!(MIB_TCP6ROW_OWNER_PID, dwLocalPort),
        std::mem::offset_of!(MIB_TCP6ROW_OWNER_PID, dwOwningPid),
    )?);
    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

fn table_pids(
    family: u32,
    port: u16,
    rows_offset: usize,
    row_size: usize,
    port_offset: usize,
    pid_offset: usize,
) -> Result<Vec<u32>, ()> {
    let mut size = 0u32;
    // u32 storage satisfies the alignment of both Windows table structures.
    let mut storage = Vec::<u32>::new();
    for _ in 0..4 {
        let pointer = if storage.is_empty() {
            std::ptr::null_mut()
        } else {
            storage.as_mut_ptr().cast()
        };
        let result = unsafe {
            GetExtendedTcpTable(
                pointer,
                &mut size,
                0,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        match result {
            ERROR_INSUFFICIENT_BUFFER if (4..=16 * 1024 * 1024).contains(&size) => {
                storage.resize((size as usize).div_ceil(4), 0);
            }
            NO_ERROR if size >= 4 && size as usize <= storage.len() * 4 => {
                // The API succeeded and reported no more than the allocated buffer.
                let bytes = unsafe {
                    std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), size as usize)
                };
                return parse_table(bytes, port, rows_offset, row_size, port_offset, pid_offset);
            }
            _ => return Err(()),
        }
    }
    Err(())
}

fn parse_table(
    bytes: &[u8],
    port: u16,
    rows_offset: usize,
    row_size: usize,
    port_offset: usize,
    pid_offset: usize,
) -> Result<Vec<u32>, ()> {
    let read = |offset: usize| -> Result<u32, ()> {
        let end = offset.checked_add(4).ok_or(())?;
        Ok(u32::from_ne_bytes(
            bytes
                .get(offset..end)
                .ok_or(())?
                .try_into()
                .map_err(|_| ())?,
        ))
    };
    let count = read(0)? as usize;
    let end = rows_offset
        .checked_add(count.checked_mul(row_size).ok_or(())?)
        .ok_or(())?;
    if end > bytes.len() {
        return Err(());
    }
    let mut pids = Vec::new();
    for row in 0..count {
        let offset = rows_offset + row * row_size;
        if u16::from_be(read(offset + port_offset)? as u16) == port {
            let pid = read(offset + pid_offset)?;
            if pid == 0 {
                return Err(());
            }
            pids.push(pid);
        }
    }
    Ok(pids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_bounds_and_network_port_order() {
        let words = [1u32, u16::to_be(18000) as u32, 42];
        let bytes: Vec<_> = words.into_iter().flat_map(u32::to_ne_bytes).collect();
        assert_eq!(parse_table(&bytes, 18000, 4, 8, 0, 4), Ok(vec![42]));
        assert_eq!(parse_table(&bytes, 18001, 4, 8, 0, 4), Ok(vec![]));
        assert_eq!(parse_table(&bytes[..11], 18000, 4, 8, 0, 4), Err(()));
        assert_eq!(parse_table(&[], 18000, 4, 8, 0, 4), Err(()));
    }

    #[test]
    #[cfg(windows)]
    #[ignore = "helper process for the owned-process test"]
    fn owned_process_wait_helper() {
        assert_eq!(
            std::env::var("WENLAN_PROCESS_TEST_HELPER").as_deref(),
            Ok("1")
        );
        std::thread::sleep(std::time::Duration::from_secs(30));
    }

    #[test]
    #[cfg(windows)]
    fn native_process_identity_gates_termination() {
        struct OwnedChild(std::process::Child);
        impl Drop for OwnedChild {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let module = module_path!().split_once("::").unwrap().1;
        let mut child = OwnedChild(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    &format!("{module}::owned_process_wait_helper"),
                    "--ignored",
                ])
                .env("WENLAN_PROCESS_TEST_HELPER", "1")
                .env("WENLAN_NO_AUTOSTART", "1")
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let pid = child.0.id();
        let (started, args) = process_identity(pid).unwrap().unwrap();
        assert!(!args.is_empty());
        assert!(!signal_process(pid, "win32:1\nargv:[]"));
        assert!(child.0.try_wait().unwrap().is_none());
        assert!(signal_process(pid, &format!("{started}\nargv:[]")));
        assert!(!child.0.wait().unwrap().success());
        assert_eq!(process_identity(pid), Ok(None));
    }
}
