#![doc = include_str!("../README.md")]

pub mod client;
pub mod embeddings;
pub(crate) mod error;
pub mod mapping;
pub(crate) mod streaming;

pub use client::OpenAi;

// Re-export neuron-types for convenience
pub use neuron_types::{
    EmbeddingError, EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, ProviderError,
    StreamEvent, StreamHandle,
};
