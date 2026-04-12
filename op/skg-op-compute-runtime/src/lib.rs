#![deny(missing_docs)]
//! Programmable execution runtime for Skelegent agents.

/// Backend lifecycle and execution contracts.
pub mod backend;
/// Error types for compute runtime operations.
pub mod error;
/// Compute operator surface for delegating model-generated code into the runtime.
pub mod operator;
/// Execution and session profile types.
pub mod profile;
/// Python-specific backend scaffolding.
pub mod python;
/// Structured execution reporting types.
pub mod report;
/// Session-scoped compute runtime trait.
pub mod runtime;
/// Compute session metadata.
pub mod session;

pub use backend::{BackendError, BackendExecRequest, BackendExecResponse, ComputeBackend};
pub use error::ComputeError;
pub use operator::{ComputeConfig, ComputeOperator};
pub use profile::{ExecutionProfile, SessionPolicy, SessionReuseMode};
pub use report::{ExecutionMetrics, ExecutionReport};
pub use runtime::ComputeRuntime;
pub use session::ComputeSession;
