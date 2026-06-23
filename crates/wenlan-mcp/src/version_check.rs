use semver::Version;

#[derive(Debug, PartialEq)]
pub enum VersionStatus {
    Compatible,
    McpOutdated { mcp: Version, daemon: Version },
    DaemonOutdated { mcp: Version, daemon: Version },
}

/// Compare wenlan-mcp's compiled version against the daemon's reported version.
/// Treats the daemon being minor/major AHEAD as `McpOutdated` (a patch-ahead
/// daemon is ignored: release-please bumps patches frequently and they're
/// API-compatible). Flags `DaemonOutdated` when the running daemon is strictly
/// OLDER than wenlan-mcp at any level, including patch (it was not restarted
/// after an upgrade). Unparseable daemon versions are treated as Compatible
/// (handshake never blocks).
pub fn compare(mcp_version: &str, daemon_version: &str) -> VersionStatus {
    // Defensive on both sides: if either version fails to parse, fall back to
    // Compatible. Never panic at startup over a malformed version string.
    let mcp = match Version::parse(mcp_version) {
        Ok(v) => v,
        Err(_) => return VersionStatus::Compatible,
    };
    let daemon = match Version::parse(daemon_version) {
        Ok(v) => v,
        Err(_) => return VersionStatus::Compatible,
    };
    if daemon.major > mcp.major || (daemon.major == mcp.major && daemon.minor > mcp.minor) {
        VersionStatus::McpOutdated { mcp, daemon }
    } else if mcp > daemon {
        // mcp strictly newer than the running daemon (any level, incl. patch):
        // the daemon binary on disk may already be new, but the running PROCESS
        // is stale — it was not restarted after an upgrade.
        VersionStatus::DaemonOutdated { mcp, daemon }
    } else {
        VersionStatus::Compatible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_minor_ahead_daemon_outdated() {
        // Was previously "Compatible"; now the daemon is flagged as stale.
        assert!(matches!(
            compare("0.2.0", "0.1.5"),
            VersionStatus::DaemonOutdated { .. }
        ));
    }

    #[test]
    fn daemon_patch_behind_outdated() {
        // The common post-upgrade case: new mcp, daemon not restarted.
        assert!(matches!(
            compare("0.8.3", "0.8.2"),
            VersionStatus::DaemonOutdated { .. }
        ));
    }

    #[test]
    fn daemon_patch_ahead_compatible() {
        // mcp slightly behind daemon by patch is fine (unchanged).
        assert_eq!(compare("0.8.2", "0.8.3"), VersionStatus::Compatible);
    }

    #[test]
    fn equal_versions_compatible() {
        assert_eq!(compare("0.8.3", "0.8.3"), VersionStatus::Compatible);
    }

    #[test]
    fn daemon_minor_ahead_outdated() {
        assert!(matches!(
            compare("0.1.2", "0.2.0"),
            VersionStatus::McpOutdated { .. }
        ));
    }

    #[test]
    fn daemon_major_ahead_outdated() {
        assert!(matches!(
            compare("0.1.2", "1.0.0"),
            VersionStatus::McpOutdated { .. }
        ));
    }

    #[test]
    fn patch_drift_compatible() {
        assert_eq!(compare("0.1.2", "0.1.5"), VersionStatus::Compatible);
    }

    #[test]
    fn unparseable_daemon_version_compatible() {
        assert_eq!(compare("0.1.2", "garbage"), VersionStatus::Compatible);
    }

    #[test]
    fn build_metadata_ignored_in_ordering() {
        // SemVer 2.0.0: build metadata after `+` is ignored when ordering versions.
        assert_eq!(compare("0.1.2", "0.1.2+abc"), VersionStatus::Compatible);
    }

    #[test]
    fn prerelease_daemon_same_minor_daemon_outdated() {
        // Daemon on a 0.2.0 pre-release while MCP is on stable 0.2.0:
        // semver ordering: 0.2.0 > 0.2.0-beta.1 (pre-releases rank lower),
        // so mcp > daemon triggers DaemonOutdated — the daemon binary is stale.
        assert!(matches!(
            compare("0.2.0", "0.2.0-beta.1"),
            VersionStatus::DaemonOutdated { .. }
        ));
    }
}
