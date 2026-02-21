//! Production runtime layer for rust-agent-blocks.
//!
//! This crate provides:
//! - [`Session`] and [`SessionState`] — conversation session management
//! - [`SessionStorage`] — trait for persisting sessions
//! - [`InMemorySessionStorage`] — in-memory session storage
//! - [`FileSessionStorage`] — file-based session storage
//! - [`SubAgentConfig`] and [`SubAgentManager`] — sub-agent spawning and isolation
//! - [`InputGuardrail`] and [`OutputGuardrail`] — safety guardrails with tripwires
//! - [`LocalDurableContext`] — passthrough durable context for local development
//! - [`Sandbox`] and [`NoOpSandbox`] — tool execution sandboxing

pub mod session;
pub mod sub_agent;
pub mod guardrail;
pub mod durable;
pub mod sandbox;

pub use session::*;
pub use sub_agent::*;
pub use guardrail::*;
pub use durable::*;
pub use sandbox::*;
