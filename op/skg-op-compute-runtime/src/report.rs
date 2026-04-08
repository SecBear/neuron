use layer0::dispatch::Artifact;
use layer0::event::ExecutionEvent;
use serde_json::Value;
use std::time::Duration;

/// Execution metrics captured during a compute run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionMetrics {
    /// Wall-clock time spent in the execution.
    pub wall_time: Duration,
    /// Number of binding calls made during execution.
    pub binding_calls: u32,
}

/// Structured execution report returned by a compute runtime.
#[derive(Debug, Clone, Default)]
pub struct ExecutionReport {
    /// Final structured result produced by the execution, if any.
    pub final_result: Option<Value>,
    /// Free-form notes captured during execution.
    pub notes: Vec<String>,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Semantic events emitted during execution.
    pub events: Vec<ExecutionEvent>,
    /// Artifacts produced during execution.
    pub artifacts: Vec<Artifact>,
    /// Execution metrics.
    pub metrics: ExecutionMetrics,
    /// Whether the execution was cancelled.
    pub cancelled: bool,
    /// Whether the execution timed out.
    pub timed_out: bool,
}
