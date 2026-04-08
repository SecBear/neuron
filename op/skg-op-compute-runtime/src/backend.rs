use crate::profile::ExecutionProfile;
use async_trait::async_trait;
use thiserror::Error;

/// Request sent to a running backend handle.
#[derive(Debug, Clone)]
pub struct BackendExecRequest {
    /// Source code to execute.
    pub code: String,
}

/// Response returned by a backend execution.
#[derive(Debug, Clone, Default)]
pub struct BackendExecResponse {
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Process exit code.
    pub exit_code: i32,
    /// Structured final result recorded by the runtime prelude, if any.
    pub final_result: Option<serde_json::Value>,
    /// Free-form notes recorded by the runtime prelude.
    pub notes: Vec<String>,
}

/// Errors produced by backend lifecycle or execution.
#[derive(Debug, Error)]
pub enum BackendError {
    /// Backend could not be started.
    #[error("backend unavailable: {0}")]
    Unavailable(String),
    /// Backend execution failed.
    #[error("backend execution failed: {0}")]
    Execution(String),
}

/// Execution backend powering a compute runtime.
#[async_trait]
pub trait ComputeBackend: Send + Sync {
    /// Opaque handle type for a running backend instance.
    type Handle: Send + Sync + 'static;

    /// Start a backend for the given profile and return a handle.
    async fn start(&self, profile: &ExecutionProfile) -> Result<Self::Handle, BackendError>;

    /// Execute a request against an existing handle.
    async fn exec(
        &self,
        handle: &Self::Handle,
        request: BackendExecRequest,
    ) -> Result<BackendExecResponse, BackendError>;

    /// Stop the backend and release resources.
    async fn stop(&self, handle: Self::Handle) -> Result<(), BackendError>;
}
