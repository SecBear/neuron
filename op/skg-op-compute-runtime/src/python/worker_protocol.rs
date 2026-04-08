//! Python worker protocol message shapes for Task 4.
use serde::{Deserialize, Serialize};

/// Requests sent to the Python worker process.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub(crate) enum WorkerRequest {
    /// Initialize the worker with a prelude to load into the namespace.
    Init { prelude: String },
    /// Execute user code in the persistent namespace.
    Exec { code: String },
    /// Reset the namespace and reinstall prelude.
    Reset,
    /// Close the worker.
    Close,
}

/// Structured response returned by the worker.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WorkerResponse {
    /// Whether the operation succeeded.
    pub ok: bool,
    /// Captured standard output from executed code (if any).
    #[serde(default)]
    pub stdout: String,
    /// Captured standard error from executed code (if any).
    #[serde(default)]
    pub stderr: String,
    /// Processed exit code (0 for success).
    #[serde(default)]
    pub exit_code: i32,
    /// Final structured value recorded via `final(value)` in prelude.
    #[serde(default)]
    pub final_result: Option<serde_json::Value>,
    /// Notes recorded via `note(text)`.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Optional error message for failed operations.
    #[serde(default)]
    pub error: Option<String>,
}