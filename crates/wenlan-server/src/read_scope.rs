// SPDX-License-Identifier: Apache-2.0

use crate::error::ServerError;
use wenlan_core::db::MemoryDB;
use wenlan_core::read_scope::{resolve_read_scope, ReadScope, ReadScopeResolveError};

pub async fn effective_read_scope(
    db: &MemoryDB,
    primary: Option<&str>,
    header: Option<&str>,
) -> Result<ReadScope, ServerError> {
    let primary = primary.map(str::trim).filter(|value| !value.is_empty());
    let header = header.map(str::trim).filter(|value| !value.is_empty());
    resolve_read_scope(db, primary.or(header))
        .await
        .map_err(map_resolve_error)
}

fn map_resolve_error(error: ReadScopeResolveError) -> ServerError {
    match error {
        ReadScopeResolveError::Unknown(name) => {
            ServerError::ValidationError(format!("unknown Space: {name}"))
        }
        ReadScopeResolveError::AmbiguousUncategorized => ServerError::ValidationError(
            "uncategorized is ambiguous with a registered Space".to_string(),
        ),
        ReadScopeResolveError::Store(error) => error.into(),
    }
}
