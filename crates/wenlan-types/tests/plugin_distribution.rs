// SPDX-License-Identifier: Apache-2.0
//! Lightweight plugin distribution contract tests.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("wenlan-types is nested under crates/")
        .to_path_buf()
}

fn read_text(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[test]
fn plugin_init_repairs_stale_daemon_versions() {
    let init = read_text("plugin/skills/init/SKILL.md");
    let hook = read_text("plugin/hooks/check-daemon.sh");

    for needle in [
        "Compare daemon version vs plugin manifest version",
        "If mismatch, repair the runtime",
        "curl -fsSL https://raw.githubusercontent.com/7xuanlu/wenlan/v${EXPECTED_VER}/install.sh | bash",
        "export PATH=\"$HOME/.wenlan/bin:$PATH\" && wenlan setup --basic && wenlan install",
    ] {
        assert!(
            init.contains(needle),
            "/init missing stale-daemon repair contract: {needle}"
        );
    }

    assert!(
        hook.contains("Run /wenlan:init to repair"),
        "SessionStart hook should direct version mismatches to the self-healing /init entrypoint"
    );
    assert!(
        !hook.contains("Otherwise upgrade: curl -fsSL"),
        "SessionStart hook should not print raw upgrade commands when /init owns repair"
    );
    assert!(
        hook.contains("for i in 1 2 3") && hook.contains("curl -fsS -m 3"),
        "SessionStart hook should retry daemon health checks to avoid false down reports"
    );
}
