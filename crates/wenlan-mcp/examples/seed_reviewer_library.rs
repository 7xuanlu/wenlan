// SPDX-License-Identifier: Apache-2.0
//! Creates a new synthetic library, never opens or replaces an existing root.
#[path = "support/reviewer_seed.rs"]
mod reviewer_seed;

use std::{path::PathBuf, sync::Arc};
use wenlan_core::{db::MemoryDB, NoopEmitter};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let requested = PathBuf::from(args.next().ok_or_else(|| {
        anyhow::anyhow!("Usage: seed_reviewer_library /absolute/new/private-root")
    })?);
    anyhow::ensure!(
        args.next().is_none() && requested.is_absolute(),
        "One absolute new root required"
    );
    let parent = requested
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Root needs a parent"))?
        .canonicalize()?;
    let root = parent.join(
        requested
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Root needs a name"))?,
    );
    #[cfg(unix)]
    let mut directory = std::fs::DirBuilder::new();
    #[cfg(not(unix))]
    let directory = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    // Exclusive creation refuses existing directories and symlinks, including
    // a partial previous seed. Failure never triggers deletion or reseeding.
    directory.create(&root)?;
    let home = root.join("home");
    let pages = root.join("pages");
    directory.create(&home)?;
    directory.create(&pages)?;
    std::env::set_var("WENLAN_NO_AUTOSTART", "1");
    std::env::set_var("WENLAN_DATA_DIR", &root);
    std::env::set_var("HOME", &home);
    std::env::set_var("USERPROFILE", &home);
    let config = wenlan_core::config::Config {
        knowledge_path: Some(pages.clone()),
        setup_completed: true,
        reranker_mode: Some("off".into()),
        ..Default::default()
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("config.json"))?;
    serde_json::to_writer_pretty(&mut file, &config)?;
    file.sync_all()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let db = MemoryDB::new(&root.join("memorydb"), Arc::new(NoopEmitter)).await?;
        reviewer_seed::seed(&db, &pages).await;
        anyhow::Ok(())
    })?;
    drop(runtime);
    println!("Synthetic reviewer library created. No listener, tunnel or deployment started.");
    Ok(())
}
