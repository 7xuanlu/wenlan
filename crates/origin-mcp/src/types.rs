//! Wire types for the Origin HTTP API.
//!
//! All types live in the published `origin-types` crate. This module exists
//! only to provide a stable import path (`crate::types::...`) during the
//! cross-repo refactor; later PRs may remove the module entirely and import
//! `origin_types::*` at call sites directly.

pub use origin_types::memory::SearchResult;
pub use origin_types::requests::{ChatContextRequest, SearchMemoryRequest, StoreMemoryRequest};
pub use origin_types::responses::{
    ChatContextResponse, DeleteResponse, SearchMemoryResponse, StoreMemoryResponse,
};
