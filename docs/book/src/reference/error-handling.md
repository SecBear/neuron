# Error Handling

All neuron error types live in `neuron-types` and use `thiserror` for
derivation. This page documents every error enum, its variants, and how to
handle them.

## Error hierarchy

```text
LoopError                       (top-level, from the agentic loop)
    |-- ProviderError           (LLM provider failures)
    |-- ToolError               (tool execution failures)
    +-- ContextError            (context compaction failures)
            +-- ProviderError   (when summarization fails)

DurableError                    (durable execution failures)
HookError                       (observability hook failures)
McpError                        (MCP protocol failures)
EmbeddingError                  (embedding provider failures)
StorageError                    (session storage failures)
SandboxError                    (sandbox execution failures)
```

`LoopError` is the primary error type you encounter when running the agentic
loop. It wraps `ProviderError`, `ToolError`, and `ContextError` via `From`
implementations, so `?` propagation works naturally.

The remaining error types (`DurableError`, `HookError`, `McpError`,
`EmbeddingError`, `StorageError`, `SandboxError`) are standalone -- they appear
in their respective subsystems and do not nest under `LoopError`.

---

## ProviderError

Errors from LLM provider operations (completions and streaming).

```rust,ignore
pub enum ProviderError {
    // --- Retryable ---
    Network(Box<dyn std::error::Error + Send + Sync>),
    RateLimit { retry_after: Option<Duration> },
    ModelLoading(String),
    Timeout(Duration),
    ServiceUnavailable(String),

    // --- Terminal ---
    Authentication(String),
    InvalidRequest(String),
    ModelNotFound(String),
    InsufficientResources(String),

    // --- Other ---
    StreamError(String),
    Other(Box<dyn std::error::Error + Send + Sync>),
}
```

### Variants

| Variant | Description | Retryable? |
|---------|-------------|------------|
| `Network` | Connection reset, DNS failure, TLS error. Wraps the underlying transport error. | Yes |
| `RateLimit` | Provider returned 429. `retry_after` contains the suggested delay if the API provided one. | Yes |
| `ModelLoading` | Model is cold-starting (common with Ollama and serverless endpoints). | Yes |
| `Timeout` | Request exceeded the configured timeout. Contains the duration that elapsed. | Yes |
| `ServiceUnavailable` | Provider returned 503 or equivalent. | Yes |
| `Authentication` | Invalid API key, expired token, or insufficient permissions (401/403). | No |
| `InvalidRequest` | Malformed request: bad parameters, unsupported model configuration, schema violations. | No |
| `ModelNotFound` | The requested model identifier does not exist on this provider. | No |
| `InsufficientResources` | Quota exceeded or billing limit reached. Distinct from rate limiting. | No |
| `StreamError` | Error during SSE streaming after the connection was established. | No |
| `Other` | Catch-all for provider-specific errors that do not fit other variants. | No |

### is_retryable()

```rust,ignore
impl ProviderError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network(_)
                | Self::RateLimit { .. }
                | Self::ModelLoading(_)
                | Self::Timeout(_)
                | Self::ServiceUnavailable(_)
        )
    }
}
```

Use `is_retryable()` to decide whether to retry a failed request. neuron does
not include built-in retry logic -- use `tower::retry`, a durable engine's retry
policy, or a simple loop:

```rust,ignore
let mut attempts = 0;
let response = loop {
    match provider.complete(request.clone()).await {
        Ok(resp) => break resp,
        Err(e) if e.is_retryable() && attempts < 3 => {
            attempts += 1;
            tokio::time::sleep(Duration::from_secs(1 << attempts)).await;
        }
        Err(e) => return Err(e),
    }
};
```

---

## EmbeddingError

Errors from embedding provider operations.

```rust,ignore
pub enum EmbeddingError {
    Authentication(String),
    RateLimit { retry_after: Option<Duration> },
    InvalidRequest(String),
    Network(Box<dyn std::error::Error + Send + Sync>),
    Other(Box<dyn std::error::Error + Send + Sync>),
}
```

### Variants

| Variant | Description | Retryable? |
|---------|-------------|------------|
| `Authentication` | Invalid API key or expired token. | No |
| `RateLimit` | Provider returned 429. | Yes |
| `InvalidRequest` | Bad input (e.g., empty input array, unsupported model). | No |
| `Network` | Connection-level failure. | Yes |
| `Other` | Catch-all. | No |

### is_retryable()

```rust,ignore
impl EmbeddingError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimit { .. } | Self::Network(_))
    }
}
```

---

## ToolError

Errors from tool operations (registration, validation, execution).

```rust,ignore
pub enum ToolError {
    NotFound(String),
    InvalidInput(String),
    ExecutionFailed(Box<dyn std::error::Error + Send + Sync>),
    PermissionDenied(String),
    Cancelled,
    ModelRetry(String),
}
```

### Variants

| Variant | Description |
|---------|-------------|
| `NotFound` | The tool name in the model's `ToolUse` block does not match any registered tool. |
| `InvalidInput` | The JSON arguments failed deserialization into the tool's `Args` type. |
| `ExecutionFailed` | The tool ran but returned an error. Wraps the tool's specific error type. |
| `PermissionDenied` | The `PermissionPolicy` denied this tool call. |
| `Cancelled` | The tool execution was cancelled via the `CancellationToken` in `ToolContext`. |
| `ModelRetry` | The tool is requesting the model to retry with different arguments. |

### ModelRetry: the self-correction pattern

`ModelRetry` is special. It does **not** propagate as an error to the caller.
Instead, the agentic loop intercepts it and converts the hint string into an
error tool result that is sent back to the model:

```rust,ignore
use neuron_types::ToolError;

// Inside a tool implementation:
fn validate_date(input: &str) -> Result<(), ToolError> {
    if !input.contains('-') {
        return Err(ToolError::ModelRetry(
            "Date must be in YYYY-MM-DD format, e.g. 2025-01-15".into()
        ));
    }
    Ok(())
}
```

The model sees the hint as a tool result with `is_error: true` and can adjust
its next tool call accordingly. This keeps self-correction logic simple: the
tool says what went wrong, and the loop handles the retry protocol.

---

## LoopError

The top-level error type returned by the agentic loop.

```rust,ignore
pub enum LoopError {
    Provider(ProviderError),
    Tool(ToolError),
    Context(ContextError),
    MaxTurns(usize),
    HookTerminated(String),
    Cancelled,
}
```

### Variants

| Variant | Description |
|---------|-------------|
| `Provider` | An LLM call failed. Check `is_retryable()` on the inner `ProviderError`. |
| `Tool` | A tool call failed (excluding `ModelRetry`, which is handled internally). |
| `Context` | Context compaction failed. |
| `MaxTurns` | The loop hit the configured turn limit. Contains the limit value. |
| `HookTerminated` | An `ObservabilityHook` returned `HookAction::Terminate`. Contains the reason. |
| `Cancelled` | The loop's cancellation token was triggered. |

### From implementations

`LoopError` implements `From<ProviderError>`, `From<ToolError>`, and
`From<ContextError>`, so you can use `?` to propagate errors from any of these
subsystems:

```rust,ignore
use neuron_types::{LoopError, ProviderError};

fn example() -> Result<(), LoopError> {
    let provider_result: Result<_, ProviderError> = Err(
        ProviderError::Authentication("invalid key".into())
    );
    provider_result?; // Automatically converted to LoopError::Provider
    Ok(())
}
```

### Handling LoopError

```rust,ignore
use neuron_types::LoopError;

match loop_result {
    Ok(response) => { /* success */ }
    Err(LoopError::Provider(e)) if e.is_retryable() => {
        // Transient provider failure -- retry the whole loop or
        // let a durable engine handle it.
    }
    Err(LoopError::Provider(e)) => {
        // Terminal provider failure -- fix config and retry.
        eprintln!("Provider error: {e}");
    }
    Err(LoopError::MaxTurns(limit)) => {
        // The agent ran for too many turns without completing.
        eprintln!("Hit {limit} turn limit");
    }
    Err(LoopError::HookTerminated(reason)) => {
        // A guardrail or hook stopped the loop.
        eprintln!("Terminated: {reason}");
    }
    Err(LoopError::Cancelled) => {
        // Graceful shutdown via cancellation token.
    }
    Err(e) => {
        eprintln!("Loop error: {e}");
    }
}
```

---

## ContextError

Errors from context management operations.

```rust,ignore
pub enum ContextError {
    CompactionFailed(String),
    Provider(ProviderError),
}
```

| Variant | Description |
|---------|-------------|
| `CompactionFailed` | The compaction strategy itself failed (e.g., produced invalid output). |
| `Provider` | A provider call during summarization-based compaction failed. Wraps `ProviderError`, so you can check `is_retryable()` on the inner error. |

---

## DurableError

Errors from durable execution operations (Temporal, Restate, Inngest).

```rust,ignore
pub enum DurableError {
    ActivityFailed(String),
    Cancelled,
    SignalTimeout,
    ContinueAsNew(String),
    Other(Box<dyn std::error::Error + Send + Sync>),
}
```

| Variant | Description |
|---------|-------------|
| `ActivityFailed` | A durable activity (LLM call or tool execution) failed after exhausting retries. |
| `Cancelled` | The workflow was cancelled externally. |
| `SignalTimeout` | `wait_for_signal()` timed out waiting for an external signal. |
| `ContinueAsNew` | The workflow needs to continue as a new execution to avoid history bloat. |
| `Other` | Catch-all for engine-specific errors. |

---

## McpError

Errors from MCP (Model Context Protocol) operations.

```rust,ignore
pub enum McpError {
    Connection(String),
    Initialization(String),
    ToolCall(String),
    Transport(String),
    Other(Box<dyn std::error::Error + Send + Sync>),
}
```

| Variant | Description |
|---------|-------------|
| `Connection` | Failed to connect to the MCP server. |
| `Initialization` | The MCP handshake (`initialize` / `initialized`) failed. |
| `ToolCall` | An MCP `tools/call` request failed. |
| `Transport` | Transport-level error (stdio pipe broken, HTTP connection dropped). |
| `Other` | Catch-all. |

---

## HookError

Errors from observability hooks.

```rust,ignore
pub enum HookError {
    Failed(String),
    Other(Box<dyn std::error::Error + Send + Sync>),
}
```

| Variant | Description |
|---------|-------------|
| `Failed` | The hook encountered an error during execution. |
| `Other` | Catch-all for hook-specific errors. |

Hook errors do not stop the loop by default. The loop logs them and continues.
To stop the loop from a hook, return `HookAction::Terminate` instead of
returning an error.

---

## StorageError

Errors from session storage operations.

```rust,ignore
pub enum StorageError {
    NotFound(String),
    Serialization(String),
    Io(std::io::Error),
    Other(Box<dyn std::error::Error + Send + Sync>),
}
```

| Variant | Description |
|---------|-------------|
| `NotFound` | The requested session does not exist in storage. |
| `Serialization` | Failed to serialize or deserialize session data. |
| `Io` | Filesystem I/O error (for file-based storage backends). |
| `Other` | Catch-all for backend-specific errors. |

---

## SandboxError

Errors from sandbox operations (isolated tool execution environments).

```rust,ignore
pub enum SandboxError {
    ExecutionFailed(String),
    SetupFailed(String),
    Other(Box<dyn std::error::Error + Send + Sync>),
}
```

| Variant | Description |
|---------|-------------|
| `ExecutionFailed` | Tool execution failed within the sandbox. |
| `SetupFailed` | Sandbox creation or teardown failed. |
| `Other` | Catch-all. |

---

## Design principles

**Two levels max.** Error enums are at most two levels deep. `LoopError::Context`
wraps `ContextError`, which wraps `ProviderError`. There is no deeper nesting.
This keeps match arms readable.

**`thiserror` everywhere.** Every error enum derives `thiserror::Error`. Display
messages are concise and include the variant's data. Source errors are linked
with `#[source]` or `#[from]` for proper error chain reporting.

**Retryable classification at the source.** `ProviderError` and `EmbeddingError`
provide `is_retryable()` because they know which failures are transient. Callers
do not need to pattern-match on specific variants to decide whether to retry.

**No built-in retry.** neuron exposes `is_retryable()` but does not include
retry middleware. Use `tower::retry`, a durable engine's retry policy, or write
a simple loop. Retry logic is inherently policy-specific (backoff strategy, max
attempts, circuit breaking) and belongs in the application layer.

**ModelRetry is not an error.** Despite living in `ToolError`, `ModelRetry` is a
control flow signal, not a failure. The loop intercepts it before it reaches
the caller. If you handle `ToolError` directly (outside the loop), treat
`ModelRetry` as a hint to feed back to the model, not as an error to log.
