// SPDX-License-Identifier: Apache-2.0
//! `origin space list/add/default/move/show` — manage memory spaces.

use anyhow::Result;
use clap::Subcommand;

use crate::client::OriginClient;
use crate::output::OutputFormat;

fn set_default_in_toml(name: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let home =
        std::env::var("HOME").map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
    let path = std::path::PathBuf::from(home).join(".origin/spaces.toml");
    std::fs::create_dir_all(path.parent().unwrap())?;

    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut found = false;
    let mut new_body = String::with_capacity(existing.len() + 32);
    for line in existing.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("default") && trimmed.contains('=') {
            new_body.push_str(&format!("default = \"{}\"\n", name));
            found = true;
        } else {
            new_body.push_str(line);
            new_body.push('\n');
        }
    }
    if !found {
        if !new_body.is_empty() && !new_body.ends_with("\n\n") {
            new_body.push('\n');
        }
        new_body.push_str(&format!("default = \"{}\"\n", name));
    }

    let mut f = std::fs::File::create(&path)?;
    f.write_all(new_body.as_bytes())?;
    Ok(())
}

fn read_default_from_toml() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home).join(".origin/spaces.toml");
    let body = std::fs::read_to_string(&path).ok()?;
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("default") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let val = rest.trim().trim_matches('"').to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
    }
    None
}

#[derive(Subcommand)]
pub enum SpaceCmd {
    /// List all registered spaces.
    List,
    /// Register a new space.
    Add {
        /// Space name (e.g. "career", "health", "ideas").
        name: String,
        /// Also set this space as the default.
        #[arg(long)]
        default: bool,
    },
    /// Get or set the default space.
    Default {
        /// Space name to set as default. Omit to print the current default.
        name: Option<String>,
    },
    /// Bulk-reassign all memories from one space to another.
    Move {
        /// Source space.
        from: String,
        /// Destination space.
        to: String,
    },
    /// Show detail for a space — memory count, page count, last activity.
    Show {
        /// Space name.
        name: String,
    },
}

pub async fn run(
    client: &OriginClient,
    format: OutputFormat,
    quiet: bool,
    cmd: SpaceCmd,
) -> Result<()> {
    match cmd {
        SpaceCmd::List => list(client, format, quiet).await,
        SpaceCmd::Add { name, default } => add(client, format, quiet, &name, default).await,
        SpaceCmd::Default { name } => default_cmd(client, format, quiet, name.as_deref()).await,
        SpaceCmd::Move { from, to } => move_cmd(client, format, quiet, &from, &to).await,
        SpaceCmd::Show { name } => show(client, format, quiet, &name).await,
    }
}

async fn list(client: &OriginClient, format: OutputFormat, quiet: bool) -> Result<()> {
    let spaces = client.list_spaces().await?;
    let default = read_default_from_toml();
    if quiet {
        return Ok(());
    }
    match format {
        OutputFormat::Json => crate::output::print_json(&spaces)?,
        OutputFormat::Table => {
            if spaces.is_empty() {
                println!("(no spaces registered)");
                return Ok(());
            }
            println!(
                "{:<20} {:<10} {:<10} {:<8}",
                "NAME", "MEMORIES", "ENTITIES", "DEFAULT?"
            );
            for s in &spaces {
                let is_default = default.as_deref() == Some(s.name.as_str());
                println!(
                    "{:<20} {:<10} {:<10} {:<8}",
                    s.name,
                    s.memory_count,
                    s.entity_count,
                    if is_default { "yes" } else { "" }
                );
            }
        }
        OutputFormat::Auto => unreachable!("Auto resolved by main before dispatch"),
    }
    Ok(())
}
async fn add(
    client: &OriginClient,
    _format: OutputFormat,
    quiet: bool,
    name: &str,
    set_default: bool,
) -> Result<()> {
    client.create_space(name).await?;
    if set_default {
        set_default_in_toml(name)?;
    }
    if !quiet {
        println!("Registered space '{}'.", name);
        if set_default {
            println!("Set '{}' as the default in ~/.origin/spaces.toml.", name);
        }
    }
    Ok(())
}
async fn default_cmd(
    _client: &OriginClient,
    _format: OutputFormat,
    quiet: bool,
    name: Option<&str>,
) -> Result<()> {
    match name {
        Some(n) => {
            set_default_in_toml(n)?;
            if !quiet {
                println!("Set default space to '{}' in ~/.origin/spaces.toml.", n);
            }
        }
        None => match read_default_from_toml() {
            Some(n) => {
                if !quiet {
                    println!("{}", n);
                }
            }
            None => {
                if !quiet {
                    println!("(no default space set; resolver layer-6 fallback is \"personal\")");
                }
            }
        },
    }
    Ok(())
}
async fn move_cmd(
    client: &OriginClient,
    _format: OutputFormat,
    quiet: bool,
    from: &str,
    to: &str,
) -> Result<()> {
    let n = client.move_space(from, to).await?;
    if !quiet {
        println!("Moved {} memories from '{}' to '{}'.", n, from, to);
    }
    Ok(())
}
async fn show(client: &OriginClient, format: OutputFormat, quiet: bool, name: &str) -> Result<()> {
    let space = client.get_space(name).await?;
    let default = read_default_from_toml();
    if quiet {
        return Ok(());
    }
    match format {
        OutputFormat::Json => crate::output::print_json(&space)?,
        OutputFormat::Table => {
            println!("Name:           {}", space.name);
            if let Some(desc) = &space.description {
                println!("Description:    {}", desc);
            }
            println!("Memory count:   {}", space.memory_count);
            println!("Entity count:   {}", space.entity_count);
            if space.starred {
                println!("Starred:        yes");
            }
            if default.as_deref() == Some(space.name.as_str()) {
                println!("Default:        yes (~/.origin/spaces.toml)");
            }
        }
        OutputFormat::Auto => unreachable!("Auto resolved by main before dispatch"),
    }
    Ok(())
}
