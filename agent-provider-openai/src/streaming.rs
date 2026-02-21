//! SSE streaming support for the OpenAI Chat Completions API.
//!
//! Parses the Server-Sent Events stream produced by OpenAI and maps events
//! to [`StreamEvent`] variants.
//!
//! Reference: <https://platform.openai.com/docs/api-reference/chat/streaming>
