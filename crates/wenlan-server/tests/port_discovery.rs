// SPDX-License-Identifier: Apache-2.0
//! Integration test: spawn origin-server with WENLAN_BIND_ADDR=127.0.0.1:0 and verify
//! both port-discovery channels (stdout printline + WENLAN_PORT_FILE) work.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn binary_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_wenlan-server"))
}

#[test]
fn port_discovery_via_stdout() {
    let tmp = tempfile::tempdir().unwrap();
    let mut child = Command::new(binary_path())
        .env("WENLAN_BIND_ADDR", "127.0.0.1:0")
        .env("WENLAN_DATA_DIR", tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn origin-server");

    let stdout = child.stdout.take().expect("stdout pipe");
    let mut reader = BufReader::new(stdout);

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut line = String::new();
    loop {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("timed out waiting for WENLAN_LISTENING_ON line");
        }
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 {
            let _ = child.kill();
            panic!("daemon stdout closed before announcing port");
        }
        if line.starts_with("WENLAN_LISTENING_ON=") {
            let addr = line
                .trim_end()
                .strip_prefix("WENLAN_LISTENING_ON=")
                .unwrap();
            assert!(addr.starts_with("127.0.0.1:"), "bad addr: {}", addr);
            let port_str = addr.split(':').next_back().unwrap();
            let _port: u16 = port_str.parse().expect("port number");
            break;
        }
    }

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn port_discovery_via_port_file() {
    let tmp = tempfile::tempdir().unwrap();
    let port_file = tmp.path().join("port");
    let mut child = Command::new(binary_path())
        .env("WENLAN_BIND_ADDR", "127.0.0.1:0")
        .env("WENLAN_DATA_DIR", tmp.path().join("data"))
        .env("WENLAN_PORT_FILE", &port_file)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn origin-server");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("timed out waiting for WENLAN_PORT_FILE");
        }
        if let Ok(contents) = std::fs::read_to_string(&port_file) {
            let _port: u16 = contents.trim().parse().expect("valid port in file");
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = child.kill();
    let _ = child.wait();
}
