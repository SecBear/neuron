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

pub mod compile;
pub mod context;
pub mod error;
pub mod middleware;
pub mod pipeline;

pub use compile::{CompileConfig, CompiledContext, InferResult};
pub use context::{Context, Extensions, TurnMetrics};
pub use error::EngineError;
pub use middleware::{ErasedMiddleware, Middleware, MiddlewareFn, middleware_fn};
pub use pipeline::Pipeline;
