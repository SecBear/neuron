//! Session management: types and storage traits.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use agent_types::{Message, StorageError, TokenUsage, WasmCompatSend, WasmCompatSync};

/// A conversation session with its messages and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique identifier for this session.
    pub id: String,
    /// The conversation messages.
    pub messages: Vec<Message>,
    /// Runtime state for this session.
    pub state: SessionState,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
}

impl Session {
    /// Create a new session with the given ID and working directory.
    #[must_use]
    pub fn new(id: impl Into<String>, cwd: PathBuf) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            messages: Vec::new(),
            state: SessionState {
                cwd,
                token_usage: TokenUsage::default(),
                event_count: 0,
                custom: HashMap::new(),
            },
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a summary of this session (without messages).
    #[must_use]
    pub fn summary(&self) -> SessionSummary {
        SessionSummary {
            id: self.id.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            message_count: self.messages.len(),
        }
    }
}

/// Mutable runtime state within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    /// Current working directory.
    pub cwd: PathBuf,
    /// Cumulative token usage across the session.
    pub token_usage: TokenUsage,
    /// Number of events processed.
    pub event_count: u64,
    /// Custom key-value metadata.
    pub custom: HashMap<String, serde_json::Value>,
}

/// A lightweight summary of a session (without messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Unique session identifier.
    pub id: String,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// Number of messages in the session.
    pub message_count: usize,
}

/// Trait for persisting and loading sessions.
///
/// # Example
///
/// ```ignore
/// use agent_runtime::*;
///
/// let storage = InMemorySessionStorage::new();
/// let session = Session::new("s-1", "/tmp".into());
/// storage.save(&session).await?;
/// let loaded = storage.load("s-1").await?;
/// assert_eq!(loaded.id, "s-1");
/// ```
pub trait SessionStorage: WasmCompatSend + WasmCompatSync {
    /// Save a session (create or update).
    fn save(
        &self,
        session: &Session,
    ) -> impl Future<Output = Result<(), StorageError>> + WasmCompatSend;

    /// Load a session by ID.
    fn load(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Session, StorageError>> + WasmCompatSend;

    /// List all session summaries.
    fn list(
        &self,
    ) -> impl Future<Output = Result<Vec<SessionSummary>, StorageError>> + WasmCompatSend;

    /// Delete a session by ID.
    fn delete(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<(), StorageError>> + WasmCompatSend;
}

/// In-memory session storage backed by a concurrent hash map.
///
/// Suitable for testing and short-lived processes.
#[derive(Debug, Clone)]
pub struct InMemorySessionStorage {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl InMemorySessionStorage {
    /// Create a new empty in-memory storage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for InMemorySessionStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStorage for InMemorySessionStorage {
    async fn save(&self, session: &Session) -> Result<(), StorageError> {
        let mut map = self.sessions.write().await;
        map.insert(session.id.clone(), session.clone());
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Session, StorageError> {
        let map = self.sessions.read().await;
        map.get(id)
            .cloned()
            .ok_or_else(|| StorageError::NotFound(id.to_string()))
    }

    async fn list(&self) -> Result<Vec<SessionSummary>, StorageError> {
        let map = self.sessions.read().await;
        Ok(map.values().map(|s| s.summary()).collect())
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let mut map = self.sessions.write().await;
        map.remove(id)
            .ok_or_else(|| StorageError::NotFound(id.to_string()))?;
        Ok(())
    }
}

/// File-based session storage storing one JSON file per session.
///
/// Each session is stored at `{directory}/{session_id}.json`.
#[derive(Debug, Clone)]
pub struct FileSessionStorage {
    directory: PathBuf,
}

impl FileSessionStorage {
    /// Create a new file-based storage at the given directory.
    ///
    /// The directory will be created if it does not exist on the first `save()`.
    #[must_use]
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    /// Compute the file path for a session.
    fn path_for(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.json"))
    }
}

impl SessionStorage for FileSessionStorage {
    async fn save(&self, session: &Session) -> Result<(), StorageError> {
        tokio::fs::create_dir_all(&self.directory).await?;
        let json = serde_json::to_string_pretty(session)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        tokio::fs::write(self.path_for(&session.id), json).await?;
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Session, StorageError> {
        let path = self.path_for(id);
        let data = tokio::fs::read_to_string(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(id.to_string())
            } else {
                StorageError::Io(e)
            }
        })?;
        let session: Session = serde_json::from_str(&data)
            .map_err(|e| StorageError::Serialization(e.to_string()))?;
        Ok(session)
    }

    async fn list(&self) -> Result<Vec<SessionSummary>, StorageError> {
        let mut summaries = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.directory).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(summaries),
            Err(e) => return Err(StorageError::Io(e)),
        };
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let data = tokio::fs::read_to_string(&path).await?;
                if let Ok(session) = serde_json::from_str::<Session>(&data) {
                    summaries.push(session.summary());
                }
            }
        }
        Ok(summaries)
    }

    async fn delete(&self, id: &str) -> Result<(), StorageError> {
        let path = self.path_for(id);
        tokio::fs::remove_file(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                StorageError::NotFound(id.to_string())
            } else {
                StorageError::Io(e)
            }
        })
    }
}
