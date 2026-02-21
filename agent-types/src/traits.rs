//! Core traits: Provider, Tool, ToolDyn, ContextStrategy, ObservabilityHook, DurableContext.

use std::future::Future;

use crate::error::ProviderError;
use crate::stream::StreamHandle;
use crate::types::{CompletionRequest, CompletionResponse};
use crate::wasm::{WasmCompatSend, WasmCompatSync};

/// LLM provider trait. Implement this for each provider (Anthropic, OpenAI, Ollama, etc.).
///
/// Uses RPITIT (return position impl trait in trait) — Rust 2024 native async.
/// Not object-safe by design; use generics `<P: Provider>` to compose.
///
/// # Example
///
/// ```ignore
/// struct MyProvider;
///
/// impl Provider for MyProvider {
///     fn complete(&self, request: CompletionRequest)
///         -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send
///     {
///         async { todo!() }
///     }
///
///     fn complete_stream(&self, request: CompletionRequest)
///         -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send
///     {
///         async { todo!() }
///     }
/// }
/// ```
pub trait Provider: WasmCompatSend + WasmCompatSync {
    /// Send a completion request and get a full response.
    fn complete(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + WasmCompatSend;

    /// Send a completion request and get a stream of events.
    fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + WasmCompatSend;
}
