//! Wire types for the Origin HTTP API.
//!
//! All types live in the published `origin-types` crate. This module exists
//! only to provide a stable import path (`crate::types::...`) during the
//! cross-repo refactor; later PRs may remove the module entirely and import
//! `origin_types::*` at call sites directly.

pub use origin_types::entities::EntitySuggestion;
pub use origin_types::memory::{RecentActivityItem, RejectionRecord, SearchResult};
pub use origin_types::requests::{
    ChatContextRequest, CreateConceptRequest, CreateEntityRequest, CreateRelationRequest,
    ListMemoriesRequest, SearchMemoryRequest, SearchPagesRequest, StoreMemoryRequest,
};
pub use origin_types::responses::{
    AcceptRefinementResponse, AddObservationResponse, ChatContextResponse, CreateEntityResponse,
    CreatePageResponse, CreateRelationResponse, DeleteResponse, EntitySuggestionApproveResponse,
    EntitySuggestionDismissResponse, ListMemoriesResponse, ListMemoryRevisionsResponse,
    ListPageRevisionsResponse, ListRefinementsResponse, MemoryRevisionEntry, NurtureCardsResponse,
    OrphanLink, OrphanLinksResponse, PageChangelogEntry, RejectRefinementResponse,
    RevisionAcceptResponse, SearchMemoryResponse, SearchPagesResponse, StoreMemoryResponse,
};
pub use origin_types::PageSourceWithMemory;
