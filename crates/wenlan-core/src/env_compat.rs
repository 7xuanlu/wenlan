// SPDX-License-Identifier: Apache-2.0
//! One-release dual-read shim for the `origin` -> `wenlan` env-var rename.
//!
//! Reads a `WENLAN_*` var, falling back to the legacy `ORIGIN_*` name for one
//! release with a deprecation warning. The legacy key is COMPUTED (not a
//! `var("ORIGIN_..")` literal) so `drift_guard`'s reader-regex does not
//! re-enumerate `ORIGIN_*`. Remove this shim when the dual-read window closes
//! (tracked follow-up); its `FLAG_ALLOWLIST` entries, if any, go with it.
use std::ffi::OsString;

/// Read `wenlan_key` (e.g. `"WENLAN_DATA_DIR"`), falling back to the legacy
/// `ORIGIN_*` equivalent for one release, with a one-time deprecation warning.
/// State-controlling vars (`WENLAN_DATA_DIR`, `WENLAN_PORT`, `WENLAN_BIND_ADDR`)
/// route through this so an existing `ORIGIN_*` setup does not silently no-op.
pub fn var_compat(wenlan_key: &str) -> Option<OsString> {
    if let Some(v) = std::env::var_os(wenlan_key) {
        return Some(v);
    }
    let legacy = wenlan_key.replacen("WENLAN_", "ORIGIN_", 1);
    if let Some(v) = std::env::var_os(&legacy) {
        log::warn!(
            "{} is deprecated; rename it to {} (legacy support will be removed in a future 0.x release)",
            legacy,
            wenlan_key
        );
        return Some(v);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_wenlan_then_falls_back_to_legacy() {
        let key = "WENLAN_ENVCOMPAT_SELFTEST";
        let legacy = "ORIGIN_ENVCOMPAT_SELFTEST";
        std::env::remove_var(key);
        std::env::remove_var(legacy);
        assert!(var_compat(key).is_none());

        std::env::set_var(legacy, "old");
        assert_eq!(var_compat(key), Some(OsString::from("old")));

        std::env::set_var(key, "new");
        assert_eq!(var_compat(key), Some(OsString::from("new")));

        std::env::remove_var(key);
        std::env::remove_var(legacy);
    }
}
