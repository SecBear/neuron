//! InMemoryStore — HashMap-backed StateStore for testing.

use crate::effect::Scope;
use crate::error::StateError;
use crate::state::{SearchResult, StateStore};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory state store backed by a `HashMap` behind a `RwLock`.
/// Scopes are serialized to strings as map keys for simplicity.
pub struct InMemoryStore {
    data: RwLock<HashMap<(String, String), serde_json::Value>>,
}

impl InMemoryStore {
    /// Create a new empty store.
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

fn scope_key(scope: &Scope) -> String {
    serde_json::to_string(scope).unwrap_or_default()
}

#[async_trait]
impl StateStore for InMemoryStore {
    async fn read(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<Option<serde_json::Value>, StateError> {
        let data = self.data.read().map_err(|e| StateError::Other(e.to_string().into()))?;
        Ok(data.get(&(scope_key(scope), key.to_owned())).cloned())
    }

    async fn write(
        &self,
        scope: &Scope,
        key: &str,
        value: serde_json::Value,
    ) -> Result<(), StateError> {
        let mut data = self.data.write().map_err(|e| StateError::WriteFailed(e.to_string()))?;
        data.insert((scope_key(scope), key.to_owned()), value);
        Ok(())
    }

    async fn delete(
        &self,
        scope: &Scope,
        key: &str,
    ) -> Result<(), StateError> {
        let mut data = self.data.write().map_err(|e| StateError::WriteFailed(e.to_string()))?;
        data.remove(&(scope_key(scope), key.to_owned()));
        Ok(())
    }

    async fn list(
        &self,
        scope: &Scope,
        prefix: &str,
    ) -> Result<Vec<String>, StateError> {
        let data = self.data.read().map_err(|e| StateError::Other(e.to_string().into()))?;
        let sk = scope_key(scope);
        Ok(data
            .keys()
            .filter(|(s, k)| s == &sk && k.starts_with(prefix))
            .map(|(_, k)| k.clone())
            .collect())
    }

    async fn search(
        &self,
        _scope: &Scope,
        _query: &str,
        _limit: usize,
    ) -> Result<Vec<SearchResult>, StateError> {
        // InMemoryStore doesn't support semantic search
        Ok(vec![])
    }
}
