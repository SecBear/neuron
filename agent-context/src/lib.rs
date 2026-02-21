//! Context engine with compaction strategies for rust-agent-blocks.
//!
//! This crate provides:
//! - [`TokenCounter`] — estimates token counts from messages
//! - [`SlidingWindowStrategy`] — keeps the last N non-system messages
//! - [`ToolResultClearingStrategy`] — replaces old tool results with a placeholder
//! - [`SummarizationStrategy`] — summarizes old messages using a Provider
//! - [`CompositeStrategy`] — chains strategies until token budget is met
//! - [`PersistentContext`] — structured context sections rendered into a system prompt
//! - [`SystemInjector`] — injects content on turn/token thresholds

pub mod counter;
pub mod strategies;
pub mod persistent;
pub mod injector;

pub use counter::TokenCounter;
pub use strategies::{
    BoxedStrategy, CompositeStrategy, SlidingWindowStrategy, SummarizationStrategy,
    ToolResultClearingStrategy,
};
pub use persistent::{ContextSection, PersistentContext};
pub use injector::{InjectionTrigger, SystemInjector};
