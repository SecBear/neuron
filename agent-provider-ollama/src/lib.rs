//! Ollama Chat API provider for rust-agent-blocks.
//!
//! This crate implements the [`Provider`] trait from `agent-types` for the
//! [Ollama Chat API](https://github.com/ollama/ollama/blob/main/docs/api.md#generate-a-chat-completion).
//!
//! # Usage
//!
//! ```no_run
//! use agent_provider_ollama::Ollama;
//!
//! let provider = Ollama::new()
//!     .model("llama3.2")
//!     .base_url("http://localhost:11434");
//! ```
//!
//! # Features
//!
//! - Full [`Provider`] implementation including streaming
//! - NDJSON streaming (Ollama uses newline-delimited JSON, not SSE)
//! - Tool call support with synthesized IDs (Ollama does not provide them)
//! - `keep_alive` configuration for model memory residency
//! - Error mapping from HTTP status codes to [`ProviderError`] variants

pub mod client;
pub mod error;
pub mod mapping;
pub mod streaming;

pub use client::Ollama;

// Re-export agent-types for convenience
pub use agent_types::{ProviderError, StreamEvent, StreamHandle};
