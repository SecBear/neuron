use layer0::id::SessionId;
use std::time::Instant;

/// Metadata for a compute session.
#[derive(Debug, Clone)]
pub struct ComputeSession {
    /// Session identifier.
    pub id: SessionId,
    /// Creation timestamp.
    pub created_at: Instant,
    /// Last-used timestamp.
    pub last_used_at: Instant,
    /// Stable hash of the execution profile associated with the session.
    pub profile_hash: String,
    /// Runtime kind (for example: `python`).
    pub runtime_kind: String,
}

impl ComputeSession {
    /// Create a new session record.
    pub fn new(id: SessionId, runtime_kind: impl Into<String>) -> Self {
        let now = Instant::now();
        Self {
            id,
            created_at: now,
            last_used_at: now,
            profile_hash: String::new(),
            runtime_kind: runtime_kind.into(),
        }
    }
}
