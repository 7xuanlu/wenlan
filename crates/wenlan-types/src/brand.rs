// SPDX-License-Identifier: Apache-2.0
//! Single source of truth for the user-facing product name.
//!
//! Display strings ONLY. Crate names, Rust symbols, env-var names, published
//! package ids, and on-disk paths are literals the compiler/OS/registry
//! require and are NOT derived from these constants (computing them would
//! defeat static analysis such as `drift_guard`'s flag scan).
pub const BRAND: &str = "Wenlan";
pub const BRAND_LOWER: &str = "wenlan";

#[cfg(test)]
mod tests {
    #[test]
    fn brand_is_wenlan() {
        assert_eq!(super::BRAND, "Wenlan");
        assert_eq!(super::BRAND_LOWER, "wenlan");
        assert!(!super::BRAND.eq_ignore_ascii_case("origin"));
    }
}
