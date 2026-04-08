use crate::backend::BackendError;
use thiserror::Error;

/// Errors produced by the compute runtime.
#[derive(Debug, Error)]
pub enum ComputeError {
    /// Backend lifecycle or execution error.
    #[error(transparent)]
    Backend(#[from] BackendError),
    /// Requested session was not found.
    #[error("session not found: {0}")]
    SessionNotFound(String),
    /// General execution failure.
    #[error("execution failed: {0}")]
    Execution(String),
}
