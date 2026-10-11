// SPDX-License-Identifier: Apache-2.0
//! `wenlan entities list/archive/restore/merge/alias` — knowledge-graph entity surface.

use anyhow::Result;
use clap::Subcommand;

use wenlan_types::entities::EntityStatus;
use wenlan_types::requests::ListEntitiesRequest;
use wenlan_types::responses::{EntityBulkResponse, ListEntitiesResponse};

use crate::client::WenlanClient;
use crate::output::ResolvedFormat;

#[derive(Subcommand)]
pub enum EntitiesCmd {
    /// List entities in every state, with their type, status, and memory count.
    List {
        /// Only this entity type (e.g. person, project).
        #[arg(long = "type")]
        entity_type: Option<String>,
        /// Only this state: detected, established, or archived.
        #[arg(long, value_parser = parse_status)]
        status: Option<EntityStatus>,
        /// Only entities whose name or alias contains this text.
        #[arg(long)]
        query: Option<String>,
        /// Max entities to return.
        #[arg(short, long, default_value_t = 50)]
        limit: u32,
        /// Skip this many entities first.
        #[arg(long, default_value_t = 0)]
        offset: u32,
    },
    /// Archive entities by id. They stay stored and can be restored.
    Archive {
        /// Entity ids to archive.
        #[arg(required = true)]
        ids: Vec<String>,
        /// Show what would be archived without changing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Restore archived entities by id.
    Restore {
        /// Entity ids to restore.
        #[arg(required = true)]
        ids: Vec<String>,
        /// Show what would be restored without changing anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Merge one entity into another. The loser is deleted; its observations,
    /// memory links, edges, and aliases move to the canonical.
    Merge {
        /// The entity to merge away (id or exact name) — the loser.
        loser: String,
        /// The entity to merge into (id or exact name) — the canonical.
        #[arg(long)]
        into: String,
        /// Preview the merge counts without mutating anything.
        #[arg(long)]
        dry_run: bool,
    },
    /// Declare an additional name for an entity.
    Alias {
        /// Entity id or exact name.
        entity: String,
        /// The alias to add.
        alias: String,
    },
}

pub async fn run(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    cmd: EntitiesCmd,
) -> Result<()> {
    match cmd {
        EntitiesCmd::List {
            entity_type,
            status,
            query,
            limit,
            offset,
        } => {
            let req = ListEntitiesRequest {
                entity_type,
                status,
                query,
                limit: Some(limit),
                offset: Some(offset),
                ..Default::default()
            };
            list(client, format, quiet, &req).await
        }
        EntitiesCmd::Archive { ids, dry_run } => {
            bulk(client, format, quiet, ids, dry_run, BulkAction::Archive).await
        }
        EntitiesCmd::Restore { ids, dry_run } => {
            bulk(client, format, quiet, ids, dry_run, BulkAction::Restore).await
        }
        EntitiesCmd::Merge {
            loser,
            into,
            dry_run,
        } => merge(client, format, quiet, &loser, &into, dry_run).await,
        EntitiesCmd::Alias { entity, alias } => {
            alias_cmd(client, format, quiet, &entity, &alias).await
        }
    }
}

fn parse_status(value: &str) -> Result<EntityStatus, String> {
    match value.to_lowercase().as_str() {
        "detected" => Ok(EntityStatus::Detected),
        "established" => Ok(EntityStatus::Established),
        "archived" => Ok(EntityStatus::Archived),
        _ => Err("expected detected, established, or archived".to_string()),
    }
}

async fn list(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    req: &ListEntitiesRequest,
) -> Result<()> {
    let response = client.query_entities(req).await?;
    if quiet {
        return Ok(());
    }
    match format {
        ResolvedFormat::Json => crate::output::print_json(&response)?,
        ResolvedFormat::Table => {
            if response.entities.is_empty() {
                println!("(no entities match)");
                return Ok(());
            }
            println!(
                "{:36}  {:28}  {:12}  {:11}  MEMORIES",
                "ID", "NAME", "TYPE", "STATUS"
            );
            for e in &response.entities {
                println!(
                    "{:36}  {:28}  {:12}  {:11}  {}",
                    e.id,
                    e.name,
                    e.entity_type,
                    e.status.as_str(),
                    e.memory_count
                );
            }
            let offset = u64::from(req.offset.unwrap_or(0));
            let shown_to = offset + response.entities.len() as u64;
            if shown_to < response.total {
                println!(
                    "Showing {}-{} of {}. Use --offset to see more.",
                    offset + 1,
                    shown_to,
                    response.total
                );
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum BulkAction {
    Archive,
    Restore,
}

async fn bulk(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    ids: Vec<String>,
    dry_run: bool,
    action: BulkAction,
) -> Result<()> {
    let response = match action {
        BulkAction::Archive => client.archive_entities(ids.clone(), dry_run).await?,
        BulkAction::Restore => client.restore_entities(ids.clone(), dry_run).await?,
    };
    if quiet {
        return Ok(());
    }
    match format {
        ResolvedFormat::Json => crate::output::print_json(&response)?,
        ResolvedFormat::Table => {
            for line in bulk_lines(action, &ids, &response) {
                println!("{line}");
            }
        }
    }
    Ok(())
}

/// One line per requested id (applied, or skipped because the daemon left it
/// alone), then a count line. Skipped ids are only named when the daemon
/// returned its full id list.
fn bulk_lines(action: BulkAction, ids: &[String], response: &EntityBulkResponse) -> Vec<String> {
    let (done, would, noun_state) = match action {
        BulkAction::Archive => ("Archived", "Would archive", "already archived"),
        BulkAction::Restore => ("Restored", "Would restore", "not archived"),
    };
    let verb = if response.dry_run { would } else { done };
    let mut lines: Vec<String> = response
        .entity_ids
        .iter()
        .map(|id| format!("{verb} {id}"))
        .collect();
    if response.entity_ids.len() as u64 == response.count {
        for id in ids.iter().filter(|id| !response.entity_ids.contains(id)) {
            lines.push(format!("Skipped {id} (not found, or {noun_state})"));
        }
    }
    let plural = if response.count == 1 {
        "entity"
    } else {
        "entities"
    };
    lines.push(format!("{verb} {} {plural}.", response.count));
    lines
}

/// Resolve `id_or_name` to an entity's (id, name): a direct id lookup first,
/// falling back to an exact case-insensitive name match against
/// `/api/memory/entities/list` (every live entity in scope, no by-name
/// filter on the route, so results are filtered client-side down to exact
/// name matches). Errors when zero or more than one entity matches the name.
/// `list_cache` lets callers that resolve two arguments reuse one
/// `/api/memory/entities/list` response instead of fetching it twice.
async fn resolve_entity_with(
    client: &WenlanClient,
    id_or_name: &str,
    list_cache: &mut Option<ListEntitiesResponse>,
) -> Result<(String, String)> {
    if let Some(detail) = client.get_entity(id_or_name).await? {
        return Ok((detail.entity.id, detail.entity.name));
    }
    if list_cache.is_none() {
        *list_cache = Some(client.list_entities().await?);
    }
    let name_lower = id_or_name.to_lowercase();
    let matches: Vec<_> = list_cache
        .as_ref()
        .expect("just populated above")
        .entities
        .iter()
        .filter(|e| e.name.to_lowercase() == name_lower)
        .collect();
    match matches.as_slice() {
        [] => anyhow::bail!("no entity found with id or exact name '{}'", id_or_name),
        [only] => Ok((only.id.clone(), only.name.clone())),
        many => {
            let candidates = many
                .iter()
                .map(|e| format!("{} ({})", e.id, e.name))
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!(
                "multiple entities match name '{}': {}",
                id_or_name,
                candidates
            )
        }
    }
}

async fn merge(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    loser: &str,
    into: &str,
    dry_run: bool,
) -> Result<()> {
    let mut list_cache = None;
    let (loser_id, _) = resolve_entity_with(client, loser, &mut list_cache).await?;
    let (canonical_id, _) = resolve_entity_with(client, into, &mut list_cache).await?;
    let response = client
        .merge_entity(&loser_id, canonical_id, dry_run)
        .await?;
    if quiet {
        return Ok(());
    }
    match format {
        ResolvedFormat::Json => crate::output::print_json(&response)?,
        ResolvedFormat::Table => {
            let verb = if response.applied {
                "Merged"
            } else {
                "Would merge"
            };
            println!(
                "{verb} '{}' into '{}' ({} memory links, {} observations, {} edges).",
                response.loser_name,
                response.canonical_name,
                response.memory_links,
                response.observations,
                response.edges,
            );
            if !response.aliases_added.is_empty() {
                println!("Aliases added: {}", response.aliases_added.join(", "));
            }
        }
    }
    Ok(())
}

async fn alias_cmd(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    entity: &str,
    alias: &str,
) -> Result<()> {
    let (entity_id, entity_name) = resolve_entity_with(client, entity, &mut None).await?;
    let response = client
        .add_entity_alias(&entity_id, alias.to_string())
        .await?;
    if quiet {
        return Ok(());
    }
    match format {
        ResolvedFormat::Json => crate::output::print_json(&response)?,
        ResolvedFormat::Table => {
            println!("'{}' aliases: {}", entity_name, response.aliases.join(", "));
        }
    }
    Ok(())
}
