use semver::Version;

#[derive(Debug, PartialEq)]
pub enum VersionStatus {
    Compatible,
    McpOutdated { mcp: Version, daemon: Version },
}

/// Compare origin-mcp's compiled version against the daemon's reported version.
/// Treats minor/major drift as `McpOutdated`. Patch differences are ignored
/// (release-please bumps patches frequently and they're API-compatible).
/// Unparseable daemon versions are treated as Compatible (handshake never blocks).
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
    } else {
        VersionStatus::Compatible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_versions_compatible() {
        assert_eq!(compare("0.1.2", "0.1.2"), VersionStatus::Compatible);
    }

    #[test]
    fn mcp_ahead_compatible() {
        assert_eq!(compare("0.2.0", "0.1.5"), VersionStatus::Compatible);
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
    fn prerelease_daemon_same_minor_compatible() {
        // Daemon on a 0.2.0 pre-release while MCP is on stable 0.2.0:
        // same major+minor → Compatible. Our gate is major/minor-only;
        // semver pre-release ordering is irrelevant at that granularity.
        assert_eq!(compare("0.2.0", "0.2.0-beta.1"), VersionStatus::Compatible);
    }
}
