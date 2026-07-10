use super::TestOracleError;
use crate::lint::pages::fs::{scan_page_root, EntryKind};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

const DB_FILES: [&str; 2] = ["origin_memory.db", "origin_memory.db-wal"];

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DbBytesFingerprint(BTreeMap<&'static str, Option<[u8; 32]>>);

impl fmt::Debug for DbBytesFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DbBytesFingerprint")
            .field("durable_files", &self.0.len())
            .finish()
    }
}

impl DbBytesFingerprint {
    pub(crate) fn capture(root: &Path) -> Result<Self, TestOracleError> {
        let files = DB_FILES
            .into_iter()
            .map(|name| {
                let path = root.join(name);
                let digest = if path.exists() {
                    Some(Sha256::digest(fs::read(path)?).into())
                } else {
                    None
                };
                Ok((name, digest))
            })
            .collect::<Result<BTreeMap<_, _>, std::io::Error>>()?;
        Ok(Self(files))
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PageBytesFingerprint {
    tree: BTreeMap<String, [u8; 32]>,
}

impl fmt::Debug for PageBytesFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageBytesFingerprint")
            .field("entry_count", &self.tree.len())
            .finish()
    }
}

impl PageBytesFingerprint {
    pub(crate) fn capture(root: &Path) -> Result<Self, TestOracleError> {
        let scan = scan_page_root(root)?;
        let mut tree = BTreeMap::new();
        for entry in scan.entries {
            let path = root.join(&entry.path);
            let bytes = match entry.kind {
                EntryKind::File => fs::read(path)?,
                EntryKind::Directory => b"directory".to_vec(),
                EntryKind::Symlink => fs::read_link(path)?.as_os_str().as_encoded_bytes().to_vec(),
                EntryKind::Other => b"other".to_vec(),
            };
            let mut digest = Sha256::new();
            digest.update(b"wenlan-lint-page-byte-v1");
            digest.update([entry.kind as u8]);
            digest.update(bytes);
            tree.insert(entry.path, digest.finalize().into());
        }
        Ok(Self { tree })
    }
}
