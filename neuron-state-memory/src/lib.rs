#![deny(missing_docs)]
//! In-memory implementation of layer0's StateStore trait.
//!
//! Uses a `HashMap` behind a `RwLock` for concurrent access.
//! Scopes are serialized to strings for use as key prefixes,
//! providing full scope isolation. Search always returns empty
//! (no semantic search support in the in-memory backend).

use async_trait::async_trait;
use layer0::effect::Scope;
use layer0::error::StateError;
use layer0::state::{SearchResult, StateStore};
use std::collections::HashMap;
use tokio::sync::RwLock;

/// In-memory state store backed by a `HashMap` behind a `RwLock`.
///
/// Suitable for testing, prototyping, and single-process use cases
/// where persistence across restarts is not required.
pub struct MemoryStore {
    data: RwLock<HashMap<String, serde_json::Value>>,
}

impl MemoryStore {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a composite key from scope + key to ensure isolation.
fn composite_key(scope: &Scope, key: &str) -> String {
    let scope_str = serde_json::to_string(scope).unwrap_or_else(|_| "unknown".to_string());
    format!("{scope_str}\0{key}")
}

/// Extract the user-facing key from a composite key, if it belongs to the given scope.
fn extract_key<'a>(composite: &'a str, scope_prefix: &str) -> Option<&'a str> {
    composite
        .strip_prefix(scope_prefix)
        .and_then(|rest| rest.strip_prefix('\0'))
}

#[async_trait]
impl StateStore for MemoryStore {
    async fn read(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<Option<serde_json::Value>, StateError> {
        let ck = composite_key(scope, key);
        let data = self.data.read().await;
        Ok(data.get(&ck).cloned())
    }

    async fn write(
        &self,
        scope: &Scope,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), StateError> {
        let ck = composite_key(scope, key);
        let mut data = self.data.write().await;
        data.insert(ck, value);
        Ok(())
    }

    async fn delete(&self, scope: &Scope, key: &str) -> Result<(), StateError> {
        let ck = composite_key(scope, key);
        let mut data = self.data.write().await;
        data.remove(&ck);
        Ok(())
    }

    async fn list(&self, scope: &Scope, prefix: &str) -> Result<Vec<String>, StateError> {
        let scope_prefix =
            serde_json::to_string(scope).unwrap_or_else(|_| "unknown".to_string());
        let data = self.data.read().await;
        let keys: Vec<String> = data
            .keys()
            .filter_map(|ck| {
                extract_key(ck, &scope_prefix).and_then(|k| {
                    if k.starts_with(prefix) {
                        Some(k.to_string())
                    } else {
                        None
                    }
                })
            })
            .collect();
        Ok(keys)
    }

    async fn search(
        &self,
        _scope: &Scope,
        _query: &str,
        _limit: usize,
    ) -> Result<Vec<SearchResult>, StateError> {
        // In-memory store does not support semantic search.
        Ok(vec![])
    }
}
