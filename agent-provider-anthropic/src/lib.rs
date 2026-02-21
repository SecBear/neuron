//! Anthropic Messages API provider for rust-agent-blocks.
//!
//! This crate implements the [`Provider`] trait from `agent-types` for the
//! [Anthropic Messages API](https://docs.anthropic.com/en/api/messages).
//!
//! # Usage
//!
//! ```no_run
//! use agent_provider_anthropic::Anthropic;
//!
//! let provider = Anthropic::new("your-api-key")
//!     .model("claude-opus-4-5");
//! ```
//!
//! # Features
//!
//! - Full [`Provider`] implementation including streaming
//! - Content block mapping: text, tool use/result, thinking, images
//! - System prompt support (text and structured blocks with cache control)
//! - Tool definition mapping with cache control
//! - Prompt caching via `CacheControl` on system blocks and tool definitions
//! - Error mapping from HTTP status codes to [`ProviderError`] variants

pub mod client;
pub mod error;
pub mod mapping;
pub mod streaming;

pub use client::Anthropic;

// Re-export agent-types for convenience
pub use agent_types::{ProviderError, StreamEvent, StreamHandle};
