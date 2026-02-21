//! Agentic while loop for rust-agent-blocks.
//!
//! This crate provides:
//! - [`AgentLoop`] — the core loop that drives provider + tool + context interactions
//! - [`AgentLoopBuilder`] — builder pattern for constructing an `AgentLoop`
//! - [`LoopConfig`] — configuration for the loop (system prompt, max turns, etc.)
//! - [`AgentResult`] — the final result of a loop run
//! - [`TurnResult`] — per-turn result for step-by-step iteration
//! - [`StepIterator`] — step-by-step iterator over loop turns

pub mod config;
pub mod loop_impl;
pub mod step;

pub use config::*;
pub use loop_impl::*;
pub use step::*;
