#![deny(missing_docs)]
//! # skg-context-engine
//!
//! Mutable context substrate for skelegent agents.
//!
//! ## Primitives
//!
//! - [`Context`] — mutable message buffer with extensions, metrics, intents.
//! - [`Middleware`] / [`ErasedMiddleware`] — the single abstraction for context
//!   transformation. Named structs for reusable middleware, async closures for
//!   one-offs.
//! - [`Pipeline`] — ordered before-send / after-send middleware phases.
//! - [`Context::compile()`] → [`CompiledContext`] — snapshot to inference request.

pub mod agent_behaviour;
pub mod agent_loop;

pub mod agent_event;
pub mod context_op;

pub mod compile;
pub mod context;
pub mod error;
pub mod middleware;
pub mod pipeline;
pub mod reactive_pipeline;
pub mod sync_operator;
pub mod router;

pub use compile::{CompileConfig, CompiledContext, InferResult};
pub use context::{Context, Extensions, TurnMetrics};
pub use error::EngineError;
pub use middleware::{ErasedMiddleware, Middleware, MiddlewareFn, middleware_fn};
pub use pipeline::Pipeline;
pub use reactive_pipeline::ReactivePipeline;
pub use sync_operator::{SyncOperator, SyncOperatorAdapter};
pub use agent_event::{AgentEvent, EventKind, TimeoutKind};
pub use context_op::{ContextOp, ErasedContextOp, OpResult, Trigger, on};
pub use router::Router;

pub use agent_behaviour::{AgentBehaviour, LoopDecision};
pub use agent_loop::{AgentLoop, PipelineFactory};