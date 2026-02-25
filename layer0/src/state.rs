//! The State protocol — how data persists and is retrieved across turns.

use crate::{effect::Scope, error::StateError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Protocol ③ — State
///
/// How data persists and is retrieved across turns and sessions.
///
/// Implementations:
/// - InMemoryStore: HashMap (testing, ephemeral)
/// - FsStore: filesystem (CLAUDE.md, plain files)
/// - GitStore: git-backed (versioned, auditable, mergeable)
/// - SqliteStore: embedded database
/// - PgStore: PostgreSQL (queryable, transactional)
///
/// The trait is deliberately minimal — CRUD + search + list.
/// Compaction is NOT part of this trait because compaction requires
/// coordination across protocols (the Lifecycle Interface).
/// Versioning is NOT part of this trait because not all backends
/// support it — implementations that do can expose it via
/// additional traits or methods.
#[async_trait]
pub trait StateStore: Send + Sync {
    /// Read a value by key within a scope.
    /// Returns None if the key doesn't exist.
    async fn read(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<Option<serde_json::Value>, StateError>;

    /// Write a value. Creates or overwrites.
    async fn write(
        &self,
        scope: &Scope,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), StateError>;

    /// Delete a value. No-op if key doesn't exist.
    async fn delete(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<(), StateError>;

    /// List keys under a prefix within a scope.
    async fn list(
        &self,
        scope: &Scope,
        prefix: &str,
    ) -> Result<Vec<String>, StateError>;

    /// Semantic search within a scope. Returns matching keys
    /// with relevance scores. Implementations that don't support
    /// search return an empty vec (not an error).
    async fn search(
        &self,
        scope: &Scope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StateError>;
}

/// A search result from a state store query.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The key that matched.
    pub key: String,
    /// Relevance score (higher is more relevant).
    pub score: f64,
    /// Preview/snippet of the matched content.
    /// Implementations decide what to include.
    pub snippet: Option<String>,
}

impl SearchResult {
    /// Create a new search result.
    pub fn new(key: impl Into<String>, score: f64) -> Self {
        Self {
            key: key.into(),
            score,
            snippet: None,
        }
    }
}

/// Read-only view of state, given to the operator runtime during
/// context assembly. The operator can read but cannot write — writes
/// go through Effects in OperatorOutput.
///
/// This trait exists to enforce the read/write asymmetry at the
/// type level. A Turn receives `&dyn StateReader`, not `&dyn StateStore`.
#[async_trait]
pub trait StateReader: Send + Sync {
    /// Read a value by key within a scope.
    async fn read(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<Option<serde_json::Value>, StateError>;

    /// List keys under a prefix within a scope.
    async fn list(
        &self,
        scope: &Scope,
        prefix: &str,
    ) -> Result<Vec<String>, StateError>;

    /// Semantic search within a scope.
    async fn search(
        &self,
        scope: &Scope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StateError>;
}

/// Blanket implementation: every StateStore is a StateReader.
#[async_trait]
impl<T: StateStore> StateReader for T {
    async fn read(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<Option<serde_json::Value>, StateError> {
        StateStore::read(self, scope, key).await
    }

    async fn list(
        &self,
        scope: &Scope,
        prefix: &str,
    ) -> Result<Vec<String>, StateError> {
        StateStore::list(self, scope, prefix).await
    }

    async fn search(
        &self,
        scope: &Scope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StateError> {
        StateStore::search(self, scope, query, limit).await
    }
}
