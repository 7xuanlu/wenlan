// SPDX-License-Identifier: Apache-2.0
//! Verify build.rs emits WENLAN_MIGRATIONS_HASH + WENLAN_GIT_SHA.

#[test]
fn migrations_hash_is_populated() {
    let h = env!("WENLAN_MIGRATIONS_HASH");
    assert_ne!(
        h, "missing",
        "migrations source files not found at build time"
    );
    assert!(!h.is_empty());
    assert_eq!(h.len(), 16, "expected 16-char sha256 prefix, got {}", h);
}

#[test]
fn git_sha_present_when_available() {
    // Optional: may be unset in tarball builds where .git/ is absent.
    // Just verify the macro compiles and doesn't panic; don't fail if unset.
    let _ = option_env!("WENLAN_GIT_SHA");
}
