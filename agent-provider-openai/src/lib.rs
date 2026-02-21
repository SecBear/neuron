//! OpenAI Chat Completions API provider for rust-agent-blocks.
//!
//! This crate implements the [`Provider`] trait from `agent-types` for the
//! [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat).
//!
//! # Usage
//!
//! ```no_run
//! use agent_provider_openai::OpenAi;
//!
//! let provider = OpenAi::new("your-api-key")
//!     .model("gpt-4o");
//! ```
//!
//! # Features
//!
//! - Full [`Provider`] implementation including streaming
//! - Content block mapping: text, tool use/result, images
//! - System prompt support (text and structured blocks)
//! - Tool definition mapping with function calling
//! - Structured output via `response_format` with JSON Schema
//! - Reasoning effort support for o-series models
//! - Error mapping from HTTP status codes to [`ProviderError`] variants

pub mod client;
pub mod error;
pub mod mapping;
pub mod streaming;

pub use client::OpenAi;

// Re-export agent-types for convenience
pub use agent_types::{ProviderError, StreamEvent, StreamHandle};
