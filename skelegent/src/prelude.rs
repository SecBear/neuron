//! Convenient re-imports for building agents.
//!
//! ```
//! use skelegent::prelude::*;
//! ```

// ── From layer0 ─────────────────────────────────────────────────────────────

pub use layer0::{
    // Operator protocol
    Operator,
    OperatorInput,
    OperatorOutput,
    OperatorMetadata,
    OperatorConfig,

    // Outcome family
    Outcome,
    TerminalOutcome,
    TransferOutcome,
    InterceptionKind,
    LimitReason,

    // Content and messages
    Content,
    ContentBlock,
    Message,
    MessageMeta,
    Role,

    // Dispatch
    DispatchHandle,
    DispatchEvent,
    DispatchContext,
    Artifact,
    CollectedDispatch,

    // Identity
    OperatorId,
    DispatchId,
    SessionId,
    WorkflowId,

    // Capabilities
    CapabilityDescriptor,
    CapabilityId,
    CapabilityKind,
    CapabilityFilter,
    CapabilityModality,
    CapabilitySource,
    ApprovalFacts,
    AuthFacts,
    ExecutionClass,
    SchedulingFacts,
    StreamingSupport,

    // Environment
    EnvironmentProvider,
    EnvironmentSpec,
    ProvisionedEnv,

    // Errors
    ProtocolError,
    ErrorCode,
    StateError,
    EnvError,

    // Intents
    Intent,
    IntentKind,
    Scope,
    MemoryScope,

    // Events
    ExecutionEvent,
    EventMeta,
    EventSource,

    // Wait / resume
    WaitReason,
    WaitState,
    ResumeInput,

    // State
    StateStore,
    StateReader,
    StoreOptions,

    // Misc
    DurationMs,
};

// TriggerType is defined in layer0::operator but not re-exported at layer0 root.
pub use layer0::operator::TriggerType;

// ── From skg-context-engine ─────────────────────────────────────────────────

pub use skg_context_engine::{
    // Context
    Context,

    // Reactive ops
    AgentEvent,
    EventKind,
    TimeoutKind,
    ContextOp,
    ErasedContextOp,
    OpResult,
    Trigger,
    ReactivePipeline,
    on,

    // Behaviours
    AgentLoop,
    AgentBehaviour,
    LoopDecision,
    PipelineFactory,
    Router,
    SyncOperator,
    SyncOperatorAdapter,

    // Compile
    CompileConfig,
    CompiledContext,
    InferResult,

    // Macro (behind feature in the underlying crate, but we enable it)
    skg_tool,
};

// ── From skg-turn ───────────────────────────────────────────────────────────

pub use skg_turn::{
    Provider,
    ProviderError,
    InferRequest,
    InferResponse,
    ToolCall,
};

// ── Operator helpers ────────────────────────────────────────────────────────

pub use layer0::operator::{completed_handle, failed_handle};

// ── Common external macros ──────────────────────────────────────────────────

pub use async_trait::async_trait;
