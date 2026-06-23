// SPDX-License-Identifier: Apache-2.0
//! One-time `origin` -> `wenlan` on-disk data migration (D2 safety envelope).
//!
//! Runs at daemon startup BEFORE the DB opens, so the daemon is the sole
//! writer. COPIES (never moves) the legacy dir so it is retained until the
//! operator removes it; verifies the DB copy is byte-identical (SHA-256)
//! before writing the completion marker; rolls back the partial copy and
//! aborts on any mismatch. Idempotent: a no-op once the new dir exists.
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum MigrationOutcome {
    NotNeeded,
    Migrated,
    AbortedNeedsManual(String),
}

/// Copy `old_root` -> `new_root` with verification. See module docs.
pub fn migrate_dir(old_root: &Path, new_root: &Path) -> std::io::Result<MigrationOutcome> {
    if new_root.exists() {
        return Ok(MigrationOutcome::NotNeeded); // already migrated, or a fresh install
    }
    if !old_root.exists() {
        return Ok(MigrationOutcome::NotNeeded); // nothing to migrate
    }
    copy_dir_recursive(old_root, new_root)?;

    // Verify the DB file (kept as `origin_memory.db` per D4) copied byte-for-byte.
    let old_db = old_root.join("memorydb").join("origin_memory.db");
    if old_db.exists() {
        let new_db = new_root.join("memorydb").join("origin_memory.db");
        if !new_db.exists() || sha256_file(&old_db)? != sha256_file(&new_db)? {
            let _ = std::fs::remove_dir_all(new_root); // roll back the partial copy
            return Ok(MigrationOutcome::AbortedNeedsManual(format!(
                "DB copy verification failed; legacy dir {} left intact",
                old_root.display()
            )));
        }
    }
    std::fs::write(new_root.join(".migrated-from-origin"), b"")?;
    Ok(MigrationOutcome::Migrated)
}

/// Run the migration and log/exit on failure. Convenience for the daemon's
/// startup path.
pub fn migrate_and_log(old_root: &Path, new_root: &Path) {
    match migrate_dir(old_root, new_root) {
        Ok(MigrationOutcome::Migrated) => log::warn!(
            "[migrate] copied legacy {} -> {} (legacy dir retained; remove after verifying)",
            old_root.display(),
            new_root.display()
        ),
        Ok(MigrationOutcome::AbortedNeedsManual(why)) => {
            log::error!("[migrate] ABORTED: {}", why);
            std::process::exit(1);
        }
        Ok(MigrationOutcome::NotNeeded) => {}
        Err(e) => {
            log::error!("[migrate] io error migrating {}: {}", old_root.display(), e);
            std::process::exit(1);
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_symlink() {
            continue; // e.g. ~/.origin/db — the daemon recreates it post-migration
        } else if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn sha256_file(p: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(std::fs::read(p)?);
    Ok(format!("{:x}", h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_db(root: &Path, bytes: &[u8]) {
        let mdb = root.join("memorydb");
        std::fs::create_dir_all(&mdb).unwrap();
        std::fs::write(mdb.join("origin_memory.db"), bytes).unwrap();
    }

    #[test]
    fn migrates_then_idempotent_and_non_destructive() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join("origin");
        let new = tmp.path().join("wenlan");
        seed_db(&old, b"sqlite-bytes-12345");

        assert_eq!(migrate_dir(&old, &new).unwrap(), MigrationOutcome::Migrated);
        assert!(new.join("memorydb/origin_memory.db").exists());
        assert!(new.join(".migrated-from-origin").exists());
        assert!(old.exists(), "legacy dir must be retained");
        assert_eq!(
            std::fs::read(new.join("memorydb/origin_memory.db")).unwrap(),
            b"sqlite-bytes-12345"
        );

        // Second run is a no-op (new dir present).
        assert_eq!(
            migrate_dir(&old, &new).unwrap(),
            MigrationOutcome::NotNeeded
        );
    }

    #[test]
    fn nothing_to_migrate() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            migrate_dir(&tmp.path().join("origin"), &tmp.path().join("wenlan")).unwrap(),
            MigrationOutcome::NotNeeded
        );
    }
}
