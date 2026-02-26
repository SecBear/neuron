# Phase 6: Turn Implementation Bridge — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build Layer 1 from scratch against layer0 protocol traits. Four new crates (neuron-tool, neuron-turn, neuron-context, neuron-provider-anthropic), full modularity, proven with real Anthropic Haiku calls.

**Context:** Design doc at `docs/plans/2026-02-25-phase6-turn-implementation-design.md`. All layer0 types in `layer0/src/`. Existing crates: layer0, neuron-state-memory, neuron-state-fs, neuron-env-local, neuron-orch-local, neuron-hooks.

**Architecture:** NeuronTurn implements `layer0::Turn` via a ReAct loop. Provider trait (RPITIT, not object-safe) handles LLM calls. ContextStrategy handles compaction. ToolDyn + ToolRegistry handle tools. The object-safe boundary is `layer0::Turn`.

**Tech Stack:** Rust (edition 2021), serde, async-trait, thiserror, rust_decimal, reqwest (for Anthropic HTTP)

**Build environment:**
```bash
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
export LIBRARY_PATH="/nix/store/7h6icyvqv6lqd0bcx41c8h3615rjcqb2-libiconv-109.100.2/lib"
```

**Check suite (run after each commit):**
```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

---

## Task 1: neuron-tool — Crate Scaffold + ToolError + ToolDyn Trait

**Files:**
- Create: `neuron-tool/Cargo.toml`
- Create: `neuron-tool/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "neuron-tool"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "Tool interface and registry for neuron"

[dependencies]
serde_json = "1"
thiserror = "2"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

**Step 2: Create src/lib.rs with ToolError, ToolDyn trait, and ToolRegistry**

```rust
#![deny(missing_docs)]
//! Tool interface and registry for neuron.
//!
//! Defines the [`ToolDyn`] trait for object-safe tool abstraction and
//! [`ToolRegistry`] for managing collections of tools. Any tool source
//! (local function, MCP server, HTTP endpoint) implements [`ToolDyn`].

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;

/// Errors from tool operations.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ToolError {
    /// The requested tool was not found in the registry.
    #[error("tool not found: {0}")]
    NotFound(String),

    /// Tool execution failed.
    #[error("execution failed: {0}")]
    ExecutionFailed(String),

    /// The input provided to the tool was invalid.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Catch-all for other errors.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Object-safe trait for tool implementations.
///
/// Any tool source (local function, MCP server, HTTP endpoint) implements
/// this trait. Tools are stored as `Arc<dyn ToolDyn>` in [`ToolRegistry`].
pub trait ToolDyn: Send + Sync {
    /// The tool's unique name.
    fn name(&self) -> &str;

    /// Human-readable description of what the tool does.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's input parameters.
    fn input_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given input.
    fn call(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>>;
}

/// Registry of tools available to a turn.
///
/// Holds tools as `Arc<dyn ToolDyn>` keyed by name. The turn's ReAct loop
/// uses this to look up and execute tools requested by the model.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolDyn>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites any existing tool with the same name.
    pub fn register(&mut self, tool: Arc<dyn ToolDyn>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn ToolDyn>> {
        self.tools.get(name)
    }

    /// Iterate over all registered tools.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn ToolDyn>> {
        self.tools.values()
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn tool_dyn_is_object_safe() {
        _assert_send_sync::<Arc<dyn ToolDyn>>();
    }

    #[test]
    fn tool_error_display() {
        assert_eq!(
            ToolError::NotFound("bash".into()).to_string(),
            "tool not found: bash"
        );
        assert_eq!(
            ToolError::ExecutionFailed("timeout".into()).to_string(),
            "execution failed: timeout"
        );
        assert_eq!(
            ToolError::InvalidInput("missing field".into()).to_string(),
            "invalid input: missing field"
        );
    }

    struct EchoTool;

    impl ToolDyn for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes input back"
        }
        fn input_schema(&self) -> serde_json::Value {
            json!({"type": "object"})
        }
        fn call(
            &self,
            input: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>>
        {
            Box::pin(async move { Ok(json!({"echoed": input})) })
        }
    }

    struct FailTool;

    impl ToolDyn for FailTool {
        fn name(&self) -> &str {
            "fail"
        }
        fn description(&self) -> &str {
            "Always fails"
        }
        fn input_schema(&self) -> serde_json::Value {
            json!({"type": "object"})
        }
        fn call(
            &self,
            _input: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>>
        {
            Box::pin(async { Err(ToolError::ExecutionFailed("always fails".into())) })
        }
    }

    #[test]
    fn registry_add_and_get() {
        let mut reg = ToolRegistry::new();
        assert!(reg.is_empty());

        reg.register(Arc::new(EchoTool));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("echo").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn registry_iter() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        reg.register(Arc::new(FailTool));

        let names: Vec<&str> = reg.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"echo"));
        assert!(names.contains(&"fail"));
    }

    #[tokio::test]
    async fn registry_call_tool() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let tool = reg.get("echo").unwrap();
        let result = tool.call(json!({"msg": "hello"})).await.unwrap();
        assert_eq!(result, json!({"echoed": {"msg": "hello"}}));
    }

    #[tokio::test]
    async fn registry_call_failing_tool() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(FailTool));

        let tool = reg.get("fail").unwrap();
        let result = tool.call(json!({})).await;
        assert!(result.is_err());
    }

    #[test]
    fn registry_overwrite() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        assert_eq!(reg.len(), 1);

        // Register another tool with the same name
        reg.register(Arc::new(EchoTool));
        assert_eq!(reg.len(), 1);
    }
}
```

**Step 3: Add neuron-tool to workspace**

In `Cargo.toml` (workspace root), add `"neuron-tool"` to the members list.

**Step 4: Build and test**

```bash
cargo build && cargo test -p neuron-tool && cargo clippy -p neuron-tool -- -D warnings
```

Expected: All tests pass, zero warnings.

**Step 5: Commit**

```bash
git add neuron-tool/ Cargo.toml
git commit -m "feat(neuron-tool): add ToolDyn trait, ToolError, and ToolRegistry"
```

---

## Task 2: neuron-turn — Crate Scaffold + Internal Types

**Files:**
- Create: `neuron-turn/Cargo.toml`
- Create: `neuron-turn/src/lib.rs`
- Create: `neuron-turn/src/types.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "neuron-turn"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "ReAct loop implementing layer0::Turn"

[dependencies]
layer0 = { path = "../layer0" }
neuron-tool = { path = "../neuron-tool" }
neuron-hooks = { path = "../neuron-hooks" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
thiserror = "2"
rust_decimal = { version = "1", features = ["serde-str"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

**Step 2: Create src/types.rs with all internal types**

```rust
//! Internal types for the neuron-turn ReAct loop.
//!
//! These are the internal lingua franca — not layer0 types, not
//! provider-specific types. Providers convert to/from these.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Role in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// System message (instructions).
    System,
    /// User message.
    User,
    /// Assistant (model) message.
    Assistant,
}

/// Source for image content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64-encoded image data.
    Base64 {
        /// The base64-encoded data.
        data: String,
    },
    /// URL pointing to an image.
    Url {
        /// The image URL.
        url: String,
    },
}

/// A single content part within a message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Plain text.
    Text {
        /// The text content.
        text: String,
    },
    /// A tool use request from the model.
    ToolUse {
        /// Unique identifier for this tool use.
        id: String,
        /// Name of the tool to invoke.
        name: String,
        /// Tool input parameters.
        input: serde_json::Value,
    },
    /// Result from a tool execution.
    ToolResult {
        /// The tool_use id this result corresponds to.
        tool_use_id: String,
        /// The result content.
        content: String,
        /// Whether the tool execution errored.
        is_error: bool,
    },
    /// Image content.
    Image {
        /// The image source.
        source: ImageSource,
        /// MIME type of the image.
        media_type: String,
    },
}

/// A message in the provider conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderMessage {
    /// The role of the message author.
    pub role: Role,
    /// Content parts of the message.
    pub content: Vec<ContentPart>,
}

/// JSON Schema description of a tool for the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Tool name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// JSON Schema for the tool's input.
    pub input_schema: serde_json::Value,
}

/// Request sent to a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
    /// Model to use (None = provider default).
    pub model: Option<String>,
    /// Conversation messages.
    pub messages: Vec<ProviderMessage>,
    /// Available tools.
    pub tools: Vec<ToolSchema>,
    /// Maximum output tokens.
    pub max_tokens: Option<u32>,
    /// Sampling temperature.
    pub temperature: Option<f64>,
    /// System prompt.
    pub system: Option<String>,
    /// Provider-specific config passthrough.
    #[serde(default)]
    pub extra: serde_json::Value,
}

/// Why the provider stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Model produced a final response.
    EndTurn,
    /// Model wants to use a tool.
    ToolUse,
    /// Hit the max_tokens limit.
    MaxTokens,
    /// Content was filtered by safety.
    ContentFilter,
}

/// Token usage from a single provider call.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens consumed.
    pub input_tokens: u64,
    /// Output tokens generated.
    pub output_tokens: u64,
    /// Tokens read from cache (if supported).
    pub cache_read_tokens: Option<u64>,
    /// Tokens written to cache (if supported).
    pub cache_creation_tokens: Option<u64>,
}

/// Response from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    /// Response content parts.
    pub content: Vec<ContentPart>,
    /// Why the provider stopped.
    pub stop_reason: StopReason,
    /// Token usage.
    pub usage: TokenUsage,
    /// Actual model used.
    pub model: String,
    /// Cost calculated by the provider (None if unknown).
    pub cost: Option<Decimal>,
    /// Whether the provider truncated input (telemetry only).
    pub truncated: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn role_serde_roundtrip() {
        for role in [Role::System, Role::User, Role::Assistant] {
            let json = serde_json::to_string(&role).unwrap();
            let back: Role = serde_json::from_str(&json).unwrap();
            assert_eq!(role, back);
        }
    }

    #[test]
    fn content_part_text_roundtrip() {
        let part = ContentPart::Text {
            text: "hello".into(),
        };
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json["type"], "text");
        let back: ContentPart = serde_json::from_value(json).unwrap();
        assert_eq!(part, back);
    }

    #[test]
    fn content_part_tool_use_roundtrip() {
        let part = ContentPart::ToolUse {
            id: "tu_1".into(),
            name: "bash".into(),
            input: json!({"command": "ls"}),
        };
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json["type"], "tool_use");
        let back: ContentPart = serde_json::from_value(json).unwrap();
        assert_eq!(part, back);
    }

    #[test]
    fn content_part_tool_result_roundtrip() {
        let part = ContentPart::ToolResult {
            tool_use_id: "tu_1".into(),
            content: "file.txt".into(),
            is_error: false,
        };
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json["type"], "tool_result");
        let back: ContentPart = serde_json::from_value(json).unwrap();
        assert_eq!(part, back);
    }

    #[test]
    fn content_part_image_roundtrip() {
        let part = ContentPart::Image {
            source: ImageSource::Url {
                url: "https://example.com/img.png".into(),
            },
            media_type: "image/png".into(),
        };
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json["type"], "image");
        let back: ContentPart = serde_json::from_value(json).unwrap();
        assert_eq!(part, back);
    }

    #[test]
    fn stop_reason_roundtrip() {
        for reason in [
            StopReason::EndTurn,
            StopReason::ToolUse,
            StopReason::MaxTokens,
            StopReason::ContentFilter,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            let back: StopReason = serde_json::from_str(&json).unwrap();
            assert_eq!(reason, back);
        }
    }

    #[test]
    fn provider_message_roundtrip() {
        let msg = ProviderMessage {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: "hello".into(),
            }],
        };
        let json = serde_json::to_value(&msg).unwrap();
        let back: ProviderMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg, back);
    }

    #[test]
    fn token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert!(usage.cache_read_tokens.is_none());
    }
}
```

**Step 3: Create src/lib.rs (minimal, just module declarations)**

```rust
#![deny(missing_docs)]
//! ReAct loop implementing `layer0::Turn`.
//!
//! This crate provides [`NeuronTurn`], a full-featured implementation of
//! the [`layer0::Turn`] trait. It runs a ReAct loop: call the model,
//! execute tools, repeat until done.
//!
//! Key traits defined here:
//! - [`Provider`] — LLM provider interface (not object-safe, uses RPITIT)
//! - [`ContextStrategy`] — context window management

pub mod types;

// Re-exports
pub use types::*;
```

**Step 4: Add neuron-turn to workspace**

In `Cargo.toml` (workspace root), add `"neuron-turn"` to the members list.

**Step 5: Build and test**

```bash
cargo build && cargo test -p neuron-turn && cargo clippy -p neuron-turn -- -D warnings
```

Expected: All tests pass, zero warnings.

**Step 6: Commit**

```bash
git add neuron-turn/ Cargo.toml
git commit -m "feat(neuron-turn): scaffold crate with internal types"
```

---

## Task 3: neuron-turn — Provider Trait + ProviderError

**Files:**
- Create: `neuron-turn/src/provider.rs`
- Modify: `neuron-turn/src/lib.rs`

**Step 1: Create src/provider.rs**

```rust
//! Provider trait for LLM backends.
//!
//! The [`Provider`] trait uses RPITIT (return-position `impl Trait` in traits)
//! and is intentionally NOT object-safe. The object-safe boundary is
//! `layer0::Turn` — NeuronTurn<P: Provider> implements Turn.

use crate::types::{ProviderRequest, ProviderResponse};
use std::future::Future;
use thiserror::Error;

/// Errors from LLM providers.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ProviderError {
    /// HTTP or network request failed.
    #[error("request failed: {0}")]
    RequestFailed(String),

    /// Provider rate-limited the request.
    #[error("rate limited")]
    RateLimited,

    /// Authentication/authorization failed.
    #[error("auth failed: {0}")]
    AuthFailed(String),

    /// Could not parse the provider's response.
    #[error("invalid response: {0}")]
    InvalidResponse(String),

    /// Catch-all for other errors.
    #[error("{0}")]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl ProviderError {
    /// Whether retrying this request might succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ProviderError::RateLimited | ProviderError::RequestFailed(_)
        )
    }
}

/// LLM provider interface.
///
/// Each provider (Anthropic, OpenAI, Ollama) implements this trait.
/// Provider-native features (truncation, caching, thinking blocks)
/// are handled by the provider impl using `ProviderRequest.extra`.
///
/// This trait uses RPITIT and is NOT object-safe. That's intentional —
/// `NeuronTurn<P: Provider>` is generic, and the object-safe boundary
/// is `layer0::Turn`.
pub trait Provider: Send + Sync {
    /// Send a completion request to the provider.
    fn complete(
        &self,
        request: ProviderRequest,
    ) -> impl Future<Output = Result<ProviderResponse, ProviderError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_error_display() {
        assert_eq!(
            ProviderError::RequestFailed("timeout".into()).to_string(),
            "request failed: timeout"
        );
        assert_eq!(ProviderError::RateLimited.to_string(), "rate limited");
        assert_eq!(
            ProviderError::AuthFailed("bad key".into()).to_string(),
            "auth failed: bad key"
        );
        assert_eq!(
            ProviderError::InvalidResponse("bad json".into()).to_string(),
            "invalid response: bad json"
        );
    }

    #[test]
    fn provider_error_retryable() {
        assert!(ProviderError::RateLimited.is_retryable());
        assert!(ProviderError::RequestFailed("timeout".into()).is_retryable());
        assert!(!ProviderError::AuthFailed("bad key".into()).is_retryable());
        assert!(!ProviderError::InvalidResponse("x".into()).is_retryable());
    }
}
```

**Step 2: Update src/lib.rs to include provider module**

Add to `neuron-turn/src/lib.rs`:

```rust
pub mod provider;

// Add to re-exports:
pub use provider::{Provider, ProviderError};
```

**Step 3: Build and test**

```bash
cargo test -p neuron-turn && cargo clippy -p neuron-turn -- -D warnings
```

**Step 4: Commit**

```bash
git add neuron-turn/src/
git commit -m "feat(neuron-turn): add Provider trait and ProviderError"
```

---

## Task 4: neuron-turn — ContextStrategy Trait + NeuronTurnConfig

**Files:**
- Create: `neuron-turn/src/context.rs`
- Create: `neuron-turn/src/config.rs`
- Modify: `neuron-turn/src/lib.rs`

**Step 1: Create src/context.rs**

```rust
//! Context strategy for managing the conversation window.
//!
//! The [`ContextStrategy`] trait handles client-side context compaction.
//! Provider-native truncation (e.g., OpenAI `truncation: auto`) is
//! invisible to the strategy — handled by the Provider impl internally.

use crate::types::ProviderMessage;

/// Strategy for managing context window size.
///
/// Implementations: `NoCompaction` (passthrough), `SlidingWindow`
/// (drop oldest messages), `Summarization` (future).
pub trait ContextStrategy: Send + Sync {
    /// Estimate token count for a message list.
    fn token_estimate(&self, messages: &[ProviderMessage]) -> usize;

    /// Whether compaction should run given the current messages and limit.
    fn should_compact(&self, messages: &[ProviderMessage], limit: usize) -> bool;

    /// Compact the message list. Returns a shorter list.
    fn compact(&self, messages: Vec<ProviderMessage>) -> Vec<ProviderMessage>;
}

/// A no-op context strategy that never compacts.
///
/// Useful for short conversations or when the provider handles
/// truncation natively.
pub struct NoCompaction;

impl ContextStrategy for NoCompaction {
    fn token_estimate(&self, messages: &[ProviderMessage]) -> usize {
        // Rough estimate: 4 chars per token
        messages
            .iter()
            .flat_map(|m| &m.content)
            .map(|part| {
                use crate::types::ContentPart;
                match part {
                    ContentPart::Text { text } => text.len() / 4,
                    ContentPart::ToolUse { input, .. } => input.to_string().len() / 4,
                    ContentPart::ToolResult { content, .. } => content.len() / 4,
                    ContentPart::Image { .. } => 1000, // rough image token estimate
                }
            })
            .sum()
    }

    fn should_compact(&self, _messages: &[ProviderMessage], _limit: usize) -> bool {
        false
    }

    fn compact(&self, messages: Vec<ProviderMessage>) -> Vec<ProviderMessage> {
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ContentPart, Role};

    #[test]
    fn no_compaction_never_compacts() {
        let strategy = NoCompaction;
        let messages = vec![ProviderMessage {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: "hello".into(),
            }],
        }];

        assert!(!strategy.should_compact(&messages, 100));
        let compacted = strategy.compact(messages.clone());
        assert_eq!(compacted.len(), messages.len());
    }

    #[test]
    fn no_compaction_estimates_tokens() {
        let strategy = NoCompaction;
        let messages = vec![ProviderMessage {
            role: Role::User,
            content: vec![ContentPart::Text {
                text: "a".repeat(400),
            }],
        }];

        let estimate = strategy.token_estimate(&messages);
        assert_eq!(estimate, 100); // 400 chars / 4
    }
}
```

**Step 2: Create src/config.rs**

```rust
//! Configuration for NeuronTurn.

/// Static configuration for a NeuronTurn instance.
///
/// Per-request overrides come from `TurnInput.config` (layer0's `TurnConfig`).
/// This struct holds the defaults.
pub struct NeuronTurnConfig {
    /// Base system prompt for this turn implementation.
    pub system_prompt: String,

    /// Default model identifier.
    pub default_model: String,

    /// Default maximum output tokens per provider call.
    pub default_max_tokens: u32,

    /// Default maximum ReAct loop iterations.
    pub default_max_turns: u32,
}

impl Default for NeuronTurnConfig {
    fn default() -> Self {
        Self {
            system_prompt: "You are a helpful assistant.".into(),
            default_model: String::new(),
            default_max_tokens: 4096,
            default_max_turns: 25,
        }
    }
}
```

**Step 3: Update src/lib.rs**

```rust
#![deny(missing_docs)]
//! ReAct loop implementing `layer0::Turn`.
//!
//! This crate provides [`NeuronTurn`], a full-featured implementation of
//! the [`layer0::Turn`] trait. It runs a ReAct loop: call the model,
//! execute tools, repeat until done.
//!
//! Key traits defined here:
//! - [`Provider`] — LLM provider interface (not object-safe, uses RPITIT)
//! - [`ContextStrategy`] — context window management

pub mod config;
pub mod context;
pub mod provider;
pub mod types;

// Re-exports
pub use config::NeuronTurnConfig;
pub use context::{ContextStrategy, NoCompaction};
pub use provider::{Provider, ProviderError};
pub use types::*;
```

**Step 4: Build and test**

```bash
cargo test -p neuron-turn && cargo clippy -p neuron-turn -- -D warnings
```

**Step 5: Commit**

```bash
git add neuron-turn/src/
git commit -m "feat(neuron-turn): add ContextStrategy trait and NeuronTurnConfig"
```

---

## Task 5: neuron-turn — Type Conversions (layer0 ↔ internal)

**Files:**
- Create: `neuron-turn/src/convert.rs`
- Modify: `neuron-turn/src/lib.rs`

**Step 1: Create src/convert.rs**

```rust
//! Bidirectional conversion between layer0 types and internal types.

use crate::types::{ContentPart, ImageSource, ProviderMessage, Role};
use layer0::content::{Content, ContentBlock};

/// Convert a layer0 `ContentBlock` to an internal `ContentPart`.
pub fn content_block_to_part(block: &ContentBlock) -> ContentPart {
    match block {
        ContentBlock::Text { text } => ContentPart::Text { text: text.clone() },
        ContentBlock::Image { source, media_type } => ContentPart::Image {
            source: image_source_to_internal(source),
            media_type: media_type.clone(),
        },
        ContentBlock::ToolUse { id, name, input } => ContentPart::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => ContentPart::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: content.clone(),
            is_error: *is_error,
        },
        ContentBlock::Custom { content_type, data } => {
            // Design decision: Custom blocks are JSON-stringified
            ContentPart::Text {
                text: format!(
                    "[custom:{}] {}",
                    content_type,
                    serde_json::to_string(data).unwrap_or_default()
                ),
            }
        }
        // Handle non_exhaustive future variants
        _ => ContentPart::Text {
            text: "[unknown content block]".into(),
        },
    }
}

/// Convert an internal `ContentPart` to a layer0 `ContentBlock`.
pub fn content_part_to_block(part: &ContentPart) -> ContentBlock {
    match part {
        ContentPart::Text { text } => ContentBlock::Text { text: text.clone() },
        ContentPart::Image { source, media_type } => ContentBlock::Image {
            source: image_source_to_layer0(source),
            media_type: media_type.clone(),
        },
        ContentPart::ToolUse { id, name, input } => ContentBlock::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        ContentPart::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => ContentBlock::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: content.clone(),
            is_error: *is_error,
        },
    }
}

/// Convert layer0 `Content` to a list of internal `ContentPart`s.
pub fn content_to_parts(content: &Content) -> Vec<ContentPart> {
    match content {
        Content::Text(text) => vec![ContentPart::Text { text: text.clone() }],
        Content::Blocks(blocks) => blocks.iter().map(content_block_to_part).collect(),
        // Handle non_exhaustive
        _ => vec![ContentPart::Text {
            text: "[unknown content]".into(),
        }],
    }
}

/// Convert internal `ContentPart`s to a layer0 `Content`.
pub fn parts_to_content(parts: &[ContentPart]) -> Content {
    if parts.len() == 1 {
        if let ContentPart::Text { text } = &parts[0] {
            return Content::Text(text.clone());
        }
    }
    Content::Blocks(parts.iter().map(content_part_to_block).collect())
}

/// Convert layer0 `Content` to an internal `ProviderMessage`.
pub fn content_to_user_message(content: &Content) -> ProviderMessage {
    ProviderMessage {
        role: Role::User,
        content: content_to_parts(content),
    }
}

fn image_source_to_internal(source: &layer0::content::ImageSource) -> ImageSource {
    match source {
        layer0::content::ImageSource::Base64 { data } => ImageSource::Base64 {
            data: data.clone(),
        },
        layer0::content::ImageSource::Url { url } => ImageSource::Url { url: url.clone() },
        // Handle non_exhaustive
        _ => ImageSource::Url {
            url: String::new(),
        },
    }
}

fn image_source_to_layer0(source: &ImageSource) -> layer0::content::ImageSource {
    match source {
        ImageSource::Base64 { data } => layer0::content::ImageSource::Base64 {
            data: data.clone(),
        },
        ImageSource::Url { url } => layer0::content::ImageSource::Url { url: url.clone() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_roundtrip() {
        let block = ContentBlock::Text {
            text: "hello".into(),
        };
        let part = content_block_to_part(&block);
        let back = content_part_to_block(&part);
        assert_eq!(block, back);
    }

    #[test]
    fn tool_use_roundtrip() {
        let block = ContentBlock::ToolUse {
            id: "tu_1".into(),
            name: "bash".into(),
            input: json!({"cmd": "ls"}),
        };
        let part = content_block_to_part(&block);
        let back = content_part_to_block(&part);
        assert_eq!(block, back);
    }

    #[test]
    fn tool_result_roundtrip() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "tu_1".into(),
            content: "output".into(),
            is_error: false,
        };
        let part = content_block_to_part(&block);
        let back = content_part_to_block(&part);
        assert_eq!(block, back);
    }

    #[test]
    fn image_roundtrip() {
        let block = ContentBlock::Image {
            source: layer0::content::ImageSource::Url {
                url: "https://example.com/img.png".into(),
            },
            media_type: "image/png".into(),
        };
        let part = content_block_to_part(&block);
        let back = content_part_to_block(&part);
        assert_eq!(block, back);
    }

    #[test]
    fn custom_block_becomes_text() {
        let block = ContentBlock::Custom {
            content_type: "thinking".into(),
            data: json!({"thought": "hmm"}),
        };
        let part = content_block_to_part(&block);
        match &part {
            ContentPart::Text { text } => {
                assert!(text.contains("[custom:thinking]"));
            }
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn content_text_to_parts() {
        let content = Content::text("hello");
        let parts = content_to_parts(&content);
        assert_eq!(parts.len(), 1);
        assert_eq!(
            parts[0],
            ContentPart::Text {
                text: "hello".into()
            }
        );
    }

    #[test]
    fn parts_to_content_single_text() {
        let parts = vec![ContentPart::Text {
            text: "hello".into(),
        }];
        let content = parts_to_content(&parts);
        assert_eq!(content, Content::text("hello"));
    }

    #[test]
    fn parts_to_content_multiple_blocks() {
        let parts = vec![
            ContentPart::Text {
                text: "hello".into(),
            },
            ContentPart::Text {
                text: "world".into(),
            },
        ];
        let content = parts_to_content(&parts);
        match content {
            Content::Blocks(blocks) => assert_eq!(blocks.len(), 2),
            _ => panic!("expected Blocks"),
        }
    }

    #[test]
    fn content_to_user_message_builds_correctly() {
        let content = Content::text("hi");
        let msg = content_to_user_message(&content);
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content.len(), 1);
    }
}
```

**Step 2: Update src/lib.rs**

Add to module declarations:
```rust
pub mod convert;
```

Add to re-exports:
```rust
pub use convert::{
    content_block_to_part, content_part_to_block, content_to_parts, content_to_user_message,
    parts_to_content,
};
```

**Step 3: Build and test**

```bash
cargo test -p neuron-turn && cargo clippy -p neuron-turn -- -D warnings
```

**Step 4: Commit**

```bash
git add neuron-turn/src/
git commit -m "feat(neuron-turn): add bidirectional type conversions (layer0 ↔ internal)"
```

---

## Task 6: neuron-turn — NeuronTurn Struct + ReAct Loop

This is the core task. NeuronTurn implements `layer0::Turn` with the full ReAct loop.

**Files:**
- Create: `neuron-turn/src/turn.rs`
- Modify: `neuron-turn/src/lib.rs`

**Step 1: Create src/turn.rs**

```rust
//! NeuronTurn — the ReAct loop implementing `layer0::Turn`.

use crate::config::NeuronTurnConfig;
use crate::context::ContextStrategy;
use crate::convert::{content_to_parts, content_to_user_message, parts_to_content};
use crate::provider::{Provider, ProviderError};
use crate::types::*;
use async_trait::async_trait;
use layer0::content::Content;
use layer0::effect::{Effect, LogLevel, Scope, SignalPayload};
use layer0::error::TurnError;
use layer0::hook::{HookAction, HookContext, HookPoint};
use layer0::id::{AgentId, WorkflowId};
use layer0::turn::{
    ExitReason, ToolCallRecord, Turn, TurnConfig, TurnInput, TurnMetadata, TurnOutput,
};
use layer0::DurationMs;
use neuron_hooks::HookRegistry;
use neuron_tool::{ToolDyn, ToolRegistry};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Instant;

/// Names of tools that produce Effects instead of executing locally.
const EFFECT_TOOL_NAMES: &[&str] = &[
    "write_memory",
    "delete_memory",
    "delegate",
    "handoff",
    "signal",
];

/// Resolved configuration merging defaults with per-request overrides.
struct ResolvedConfig {
    model: Option<String>,
    system: String,
    max_turns: u32,
    max_cost: Option<Decimal>,
    max_duration: Option<DurationMs>,
    allowed_tools: Option<Vec<String>>,
    max_tokens: u32,
}

/// A full-featured Turn implementation with a ReAct loop.
///
/// Generic over `P: Provider` (not object-safe). The object-safe boundary
/// is `layer0::Turn`, which `NeuronTurn<P>` implements via `#[async_trait]`.
pub struct NeuronTurn<P: Provider> {
    provider: P,
    tools: ToolRegistry,
    context_strategy: Box<dyn ContextStrategy>,
    hooks: HookRegistry,
    state_reader: Arc<dyn layer0::StateReader>,
    config: NeuronTurnConfig,
}

impl<P: Provider> NeuronTurn<P> {
    /// Create a new NeuronTurn with all dependencies.
    pub fn new(
        provider: P,
        tools: ToolRegistry,
        context_strategy: Box<dyn ContextStrategy>,
        hooks: HookRegistry,
        state_reader: Arc<dyn layer0::StateReader>,
        config: NeuronTurnConfig,
    ) -> Self {
        Self {
            provider,
            tools,
            context_strategy,
            hooks,
            state_reader,
            config,
        }
    }

    fn resolve_config(&self, input: &TurnInput) -> ResolvedConfig {
        let tc = input.config.as_ref();
        let system = match tc.and_then(|c| c.system_addendum.as_ref()) {
            Some(addendum) => format!("{}\n{}", self.config.system_prompt, addendum),
            None => self.config.system_prompt.clone(),
        };
        ResolvedConfig {
            model: tc
                .and_then(|c| c.model.clone())
                .or_else(|| {
                    if self.config.default_model.is_empty() {
                        None
                    } else {
                        Some(self.config.default_model.clone())
                    }
                }),
            system,
            max_turns: tc
                .and_then(|c| c.max_turns)
                .unwrap_or(self.config.default_max_turns),
            max_cost: tc.and_then(|c| c.max_cost),
            max_duration: tc.and_then(|c| c.max_duration),
            allowed_tools: tc.and_then(|c| c.allowed_tools.clone()),
            max_tokens: self.config.default_max_tokens,
        }
    }

    fn build_tool_schemas(&self, config: &ResolvedConfig) -> Vec<ToolSchema> {
        let mut schemas: Vec<ToolSchema> = self
            .tools
            .iter()
            .map(|tool| ToolSchema {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
            })
            .collect();

        // Add effect tool schemas
        schemas.extend(effect_tool_schemas());

        // Filter by allowed_tools if specified
        if let Some(allowed) = &config.allowed_tools {
            schemas.retain(|s| allowed.contains(&s.name));
        }

        schemas
    }

    async fn assemble_context(
        &self,
        input: &TurnInput,
    ) -> Result<Vec<ProviderMessage>, TurnError> {
        let mut messages = Vec::new();

        // Read history from state if session is present
        if let Some(session) = &input.session {
            let scope = Scope::Session(session.clone());
            match self.state_reader.read(&scope, "messages").await {
                Ok(Some(history)) => {
                    if let Ok(history_messages) =
                        serde_json::from_value::<Vec<ProviderMessage>>(history)
                    {
                        messages = history_messages;
                    }
                }
                Ok(None) => {} // No history yet
                Err(_) => {}   // State read errors are non-fatal
            }
        }

        // Add the new user message
        messages.push(content_to_user_message(&input.message));

        Ok(messages)
    }

    fn try_as_effect(&self, name: &str, input: &serde_json::Value) -> Option<Effect> {
        match name {
            "write_memory" => {
                let scope_str = input.get("scope")?.as_str()?;
                let key = input.get("key")?.as_str()?.to_string();
                let value = input.get("value")?.clone();
                let scope = parse_scope(scope_str);
                Some(Effect::WriteMemory { scope, key, value })
            }
            "delete_memory" => {
                let scope_str = input.get("scope")?.as_str()?;
                let key = input.get("key")?.as_str()?.to_string();
                let scope = parse_scope(scope_str);
                Some(Effect::DeleteMemory { scope, key })
            }
            "delegate" => {
                let agent = input.get("agent")?.as_str()?;
                let message = input
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("");
                let delegate_input = TurnInput::new(
                    Content::text(message),
                    layer0::turn::TriggerType::Task,
                );
                Some(Effect::Delegate {
                    agent: AgentId::new(agent),
                    input: Box::new(delegate_input),
                })
            }
            "handoff" => {
                let agent = input.get("agent")?.as_str()?;
                let state = input
                    .get("state")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Some(Effect::Handoff {
                    agent: AgentId::new(agent),
                    state,
                })
            }
            "signal" => {
                let target = input.get("target")?.as_str()?;
                let signal_type = input
                    .get("signal_type")
                    .and_then(|s| s.as_str())
                    .unwrap_or("default");
                let data = input
                    .get("data")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Some(Effect::Signal {
                    target: WorkflowId::new(target),
                    payload: SignalPayload::new(signal_type, data),
                })
            }
            _ => None,
        }
    }

    fn build_metadata(
        &self,
        tokens_in: u64,
        tokens_out: u64,
        cost: Decimal,
        turns_used: u32,
        tools_called: Vec<ToolCallRecord>,
        duration: DurationMs,
    ) -> TurnMetadata {
        TurnMetadata {
            tokens_in,
            tokens_out,
            cost,
            turns_used,
            tools_called,
            duration,
        }
    }

    fn build_hook_context(
        &self,
        point: HookPoint,
        tokens_in: u64,
        tokens_out: u64,
        cost: Decimal,
        turns_completed: u32,
        elapsed: DurationMs,
    ) -> HookContext {
        let mut ctx = HookContext::new(point);
        ctx.tokens_used = tokens_in + tokens_out;
        ctx.cost = cost;
        ctx.turns_completed = turns_completed;
        ctx.elapsed = elapsed;
        ctx
    }
}

#[async_trait]
impl<P: Provider + 'static> Turn for NeuronTurn<P> {
    async fn execute(&self, input: TurnInput) -> Result<TurnOutput, TurnError> {
        let start = Instant::now();
        let config = self.resolve_config(&input);
        let mut messages = self.assemble_context(&input).await?;
        let tools = self.build_tool_schemas(&config);

        let mut total_tokens_in: u64 = 0;
        let mut total_tokens_out: u64 = 0;
        let mut total_cost = Decimal::ZERO;
        let mut turns_used: u32 = 0;
        let mut tool_records: Vec<ToolCallRecord> = vec![];
        let mut effects: Vec<Effect> = vec![];
        let mut last_content: Vec<ContentPart> = vec![];

        loop {
            turns_used += 1;

            // 1. Hook: PreInference
            let hook_ctx = self.build_hook_context(
                HookPoint::PreInference,
                total_tokens_in,
                total_tokens_out,
                total_cost,
                turns_used - 1,
                DurationMs::from(start.elapsed()),
            );
            if let HookAction::Halt { reason } = self.hooks.dispatch(&hook_ctx).await {
                return Ok(TurnOutput {
                    message: parts_to_content(&last_content),
                    exit_reason: ExitReason::ObserverHalt { reason },
                    metadata: self.build_metadata(
                        total_tokens_in,
                        total_tokens_out,
                        total_cost,
                        turns_used,
                        tool_records,
                        DurationMs::from(start.elapsed()),
                    ),
                    effects,
                });
            }

            // 2. Build ProviderRequest
            let request = ProviderRequest {
                model: config.model.clone(),
                messages: messages.clone(),
                tools: tools.clone(),
                max_tokens: Some(config.max_tokens),
                temperature: None,
                system: Some(config.system.clone()),
                extra: input.metadata.clone(),
            };

            // 3. Call provider
            let response = self.provider.complete(request).await.map_err(|e| {
                if e.is_retryable() {
                    TurnError::Retryable(e.to_string())
                } else {
                    TurnError::Model(e.to_string())
                }
            })?;

            // 4. Hook: PostInference
            let mut hook_ctx = self.build_hook_context(
                HookPoint::PostInference,
                total_tokens_in + response.usage.input_tokens,
                total_tokens_out + response.usage.output_tokens,
                total_cost + response.cost.unwrap_or(Decimal::ZERO),
                turns_used,
                DurationMs::from(start.elapsed()),
            );
            hook_ctx.model_output = Some(parts_to_content(&response.content));
            if let HookAction::Halt { reason } = self.hooks.dispatch(&hook_ctx).await {
                return Ok(TurnOutput {
                    message: parts_to_content(&response.content),
                    exit_reason: ExitReason::ObserverHalt { reason },
                    metadata: self.build_metadata(
                        total_tokens_in + response.usage.input_tokens,
                        total_tokens_out + response.usage.output_tokens,
                        total_cost + response.cost.unwrap_or(Decimal::ZERO),
                        turns_used,
                        tool_records,
                        DurationMs::from(start.elapsed()),
                    ),
                    effects,
                });
            }

            // 5. Aggregate tokens + cost
            total_tokens_in += response.usage.input_tokens;
            total_tokens_out += response.usage.output_tokens;
            if let Some(cost) = response.cost {
                total_cost += cost;
            }

            last_content.clone_from(&response.content);

            // 6. Check StopReason
            match response.stop_reason {
                StopReason::MaxTokens => {
                    return Err(TurnError::Model(
                        "output truncated (max_tokens)".into(),
                    ));
                }
                StopReason::ContentFilter => {
                    return Err(TurnError::Model("content filtered".into()));
                }
                StopReason::EndTurn => {
                    return Ok(TurnOutput {
                        message: parts_to_content(&response.content),
                        exit_reason: ExitReason::Complete,
                        metadata: self.build_metadata(
                            total_tokens_in,
                            total_tokens_out,
                            total_cost,
                            turns_used,
                            tool_records,
                            DurationMs::from(start.elapsed()),
                        ),
                        effects,
                    });
                }
                StopReason::ToolUse => {
                    // Continue to tool execution below
                }
            }

            // 7. Tool execution
            // Add assistant message to context
            messages.push(ProviderMessage {
                role: Role::Assistant,
                content: response.content.clone(),
            });

            let tool_uses: Vec<(String, String, serde_json::Value)> = response
                .content
                .iter()
                .filter_map(|part| match part {
                    ContentPart::ToolUse { id, name, input } => {
                        Some((id.clone(), name.clone(), input.clone()))
                    }
                    _ => None,
                })
                .collect();

            let mut tool_results = Vec::new();

            for (id, name, tool_input) in tool_uses {
                // a. Check effect tool
                if EFFECT_TOOL_NAMES.contains(&name.as_str()) {
                    if let Some(effect) = self.try_as_effect(&name, &tool_input) {
                        effects.push(effect);
                    }
                    tool_results.push(ContentPart::ToolResult {
                        tool_use_id: id,
                        content: format!("{} effect recorded.", name),
                        is_error: false,
                    });
                    tool_records.push(ToolCallRecord::new(&name, DurationMs::ZERO, true));
                    continue;
                }

                // b. Hook: PreToolUse
                let mut actual_input = tool_input.clone();
                let mut hook_ctx = HookContext::new(HookPoint::PreToolUse);
                hook_ctx.tool_name = Some(name.clone());
                hook_ctx.tool_input = Some(tool_input.clone());
                hook_ctx.tokens_used = total_tokens_in + total_tokens_out;
                hook_ctx.cost = total_cost;
                hook_ctx.turns_completed = turns_used;
                hook_ctx.elapsed = DurationMs::from(start.elapsed());

                match self.hooks.dispatch(&hook_ctx).await {
                    HookAction::Halt { reason } => {
                        return Ok(TurnOutput {
                            message: parts_to_content(&last_content),
                            exit_reason: ExitReason::ObserverHalt { reason },
                            metadata: self.build_metadata(
                                total_tokens_in,
                                total_tokens_out,
                                total_cost,
                                turns_used,
                                tool_records,
                                DurationMs::from(start.elapsed()),
                            ),
                            effects,
                        });
                    }
                    HookAction::SkipTool { reason } => {
                        tool_results.push(ContentPart::ToolResult {
                            tool_use_id: id,
                            content: format!("Skipped: {}", reason),
                            is_error: false,
                        });
                        tool_records.push(ToolCallRecord::new(&name, DurationMs::ZERO, false));
                        continue;
                    }
                    HookAction::ModifyToolInput { new_input } => {
                        actual_input = new_input;
                    }
                    HookAction::Continue => {}
                    // Handle non_exhaustive
                    _ => {}
                }

                // c. Execute tool
                let tool_start = Instant::now();
                let result = match self.tools.get(&name) {
                    Some(tool) => tool.call(actual_input).await,
                    None => Err(neuron_tool::ToolError::NotFound(name.clone())),
                };
                let tool_duration = DurationMs::from(tool_start.elapsed());

                let (result_content, is_error, success) = match result {
                    Ok(value) => (
                        serde_json::to_string(&value).unwrap_or_default(),
                        false,
                        true,
                    ),
                    Err(e) => (e.to_string(), true, false),
                };

                // d. Hook: PostToolUse
                let mut hook_ctx = HookContext::new(HookPoint::PostToolUse);
                hook_ctx.tool_name = Some(name.clone());
                hook_ctx.tool_result = Some(result_content.clone());
                hook_ctx.tokens_used = total_tokens_in + total_tokens_out;
                hook_ctx.cost = total_cost;
                hook_ctx.turns_completed = turns_used;
                hook_ctx.elapsed = DurationMs::from(start.elapsed());

                if let HookAction::Halt { reason } = self.hooks.dispatch(&hook_ctx).await {
                    return Ok(TurnOutput {
                        message: parts_to_content(&last_content),
                        exit_reason: ExitReason::ObserverHalt { reason },
                        metadata: self.build_metadata(
                            total_tokens_in,
                            total_tokens_out,
                            total_cost,
                            turns_used,
                            tool_records,
                            DurationMs::from(start.elapsed()),
                        ),
                        effects,
                    });
                }

                // e. Backfill result
                tool_results.push(ContentPart::ToolResult {
                    tool_use_id: id,
                    content: result_content,
                    is_error,
                });

                // f. Record
                tool_records.push(ToolCallRecord::new(name, tool_duration, success));
            }

            // Add tool results as user message
            messages.push(ProviderMessage {
                role: Role::User,
                content: tool_results,
            });

            // 8. Check limits
            if turns_used >= config.max_turns {
                return Ok(TurnOutput {
                    message: parts_to_content(&last_content),
                    exit_reason: ExitReason::MaxTurns,
                    metadata: self.build_metadata(
                        total_tokens_in,
                        total_tokens_out,
                        total_cost,
                        turns_used,
                        tool_records,
                        DurationMs::from(start.elapsed()),
                    ),
                    effects,
                });
            }

            if let Some(max_cost) = &config.max_cost {
                if total_cost >= *max_cost {
                    return Ok(TurnOutput {
                        message: parts_to_content(&last_content),
                        exit_reason: ExitReason::BudgetExhausted,
                        metadata: self.build_metadata(
                            total_tokens_in,
                            total_tokens_out,
                            total_cost,
                            turns_used,
                            tool_records,
                            DurationMs::from(start.elapsed()),
                        ),
                        effects,
                    });
                }
            }

            if let Some(max_duration) = &config.max_duration {
                if start.elapsed() >= max_duration.to_std() {
                    return Ok(TurnOutput {
                        message: parts_to_content(&last_content),
                        exit_reason: ExitReason::Timeout,
                        metadata: self.build_metadata(
                            total_tokens_in,
                            total_tokens_out,
                            total_cost,
                            turns_used,
                            tool_records,
                            DurationMs::from(start.elapsed()),
                        ),
                        effects,
                    });
                }
            }

            // 9. Hook: ExitCheck
            let hook_ctx = self.build_hook_context(
                HookPoint::ExitCheck,
                total_tokens_in,
                total_tokens_out,
                total_cost,
                turns_used,
                DurationMs::from(start.elapsed()),
            );
            if let HookAction::Halt { reason } = self.hooks.dispatch(&hook_ctx).await {
                return Ok(TurnOutput {
                    message: parts_to_content(&last_content),
                    exit_reason: ExitReason::ObserverHalt { reason },
                    metadata: self.build_metadata(
                        total_tokens_in,
                        total_tokens_out,
                        total_cost,
                        turns_used,
                        tool_records,
                        DurationMs::from(start.elapsed()),
                    ),
                    effects,
                });
            }

            // 10. Context compaction
            let limit = config.max_tokens as usize * 4;
            if self.context_strategy.should_compact(&messages, limit) {
                messages = self.context_strategy.compact(messages);
            }

            // 11. Loop repeats
        }
    }
}

/// Schemas for effect tools that the model can call.
fn effect_tool_schemas() -> Vec<ToolSchema> {
    vec![
        ToolSchema {
            name: "write_memory".into(),
            description: "Write a value to persistent memory.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "scope": {"type": "string", "description": "Memory scope (e.g. 'global', 'session:id')"},
                    "key": {"type": "string", "description": "Memory key"},
                    "value": {"description": "Value to store"}
                },
                "required": ["scope", "key", "value"]
            }),
        },
        ToolSchema {
            name: "delete_memory".into(),
            description: "Delete a value from persistent memory.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "scope": {"type": "string", "description": "Memory scope"},
                    "key": {"type": "string", "description": "Memory key"}
                },
                "required": ["scope", "key"]
            }),
        },
        ToolSchema {
            name: "delegate".into(),
            description: "Delegate a task to another agent.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent": {"type": "string", "description": "Agent ID to delegate to"},
                    "message": {"type": "string", "description": "Task description for the agent"}
                },
                "required": ["agent", "message"]
            }),
        },
        ToolSchema {
            name: "handoff".into(),
            description: "Hand off the conversation to another agent.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "agent": {"type": "string", "description": "Agent ID to hand off to"},
                    "state": {"description": "State to pass to the next agent"}
                },
                "required": ["agent"]
            }),
        },
        ToolSchema {
            name: "signal".into(),
            description: "Send a signal to another workflow.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "target": {"type": "string", "description": "Target workflow ID"},
                    "signal_type": {"type": "string", "description": "Signal type identifier"},
                    "data": {"description": "Signal payload data"}
                },
                "required": ["target"]
            }),
        },
    ]
}

/// Parse a scope string into a layer0 Scope.
fn parse_scope(s: &str) -> Scope {
    if s == "global" {
        return Scope::Global;
    }
    if let Some(id) = s.strip_prefix("session:") {
        return Scope::Session(layer0::SessionId::new(id));
    }
    if let Some(id) = s.strip_prefix("workflow:") {
        return Scope::Workflow(layer0::WorkflowId::new(id));
    }
    Scope::Custom(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::NoCompaction;
    use neuron_hooks::HookRegistry;
    use neuron_tool::ToolRegistry;
    use serde_json::json;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    // ── Mock Provider ──────────────────────────────────

    struct MockProvider {
        responses: Mutex<VecDeque<ProviderResponse>>,
        call_count: AtomicUsize,
    }

    impl MockProvider {
        fn new(responses: Vec<ProviderResponse>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                call_count: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    impl Provider for MockProvider {
        fn complete(
            &self,
            _request: ProviderRequest,
        ) -> impl std::future::Future<Output = Result<ProviderResponse, ProviderError>> + Send
        {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let response = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("MockProvider: no more responses queued");
            async move { Ok(response) }
        }
    }

    // ── Mock StateReader ───────────────────────────────

    struct NullStateReader;

    #[async_trait]
    impl layer0::StateReader for NullStateReader {
        async fn read(
            &self,
            _scope: &Scope,
            _key: &str,
        ) -> Result<Option<serde_json::Value>, layer0::StateError> {
            Ok(None)
        }
        async fn list(
            &self,
            _scope: &Scope,
            _prefix: &str,
        ) -> Result<Vec<String>, layer0::StateError> {
            Ok(vec![])
        }
        async fn search(
            &self,
            _scope: &Scope,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<layer0::state::SearchResult>, layer0::StateError> {
            Ok(vec![])
        }
    }

    // ── Mock Tool ──────────────────────────────────────

    struct EchoTool;

    impl neuron_tool::ToolDyn for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes input"
        }
        fn input_schema(&self) -> serde_json::Value {
            json!({"type": "object"})
        }
        fn call(
            &self,
            input: serde_json::Value,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = Result<serde_json::Value, neuron_tool::ToolError>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async move { Ok(json!({"echoed": input})) })
        }
    }

    // ── Helpers ────────────────────────────────────────

    fn simple_text_response(text: &str) -> ProviderResponse {
        ProviderResponse {
            content: vec![ContentPart::Text {
                text: text.to_string(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                ..Default::default()
            },
            model: "mock-model".into(),
            cost: Some(Decimal::new(1, 4)), // $0.0001
            truncated: None,
        }
    }

    fn tool_use_response(tool_id: &str, tool_name: &str, input: serde_json::Value) -> ProviderResponse {
        ProviderResponse {
            content: vec![ContentPart::ToolUse {
                id: tool_id.to_string(),
                name: tool_name.to_string(),
                input,
            }],
            stop_reason: StopReason::ToolUse,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 15,
                ..Default::default()
            },
            model: "mock-model".into(),
            cost: Some(Decimal::new(2, 4)), // $0.0002
            truncated: None,
        }
    }

    fn make_turn<P: Provider>(provider: P) -> NeuronTurn<P> {
        NeuronTurn::new(
            provider,
            ToolRegistry::new(),
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            NeuronTurnConfig::default(),
        )
    }

    fn make_turn_with_tools<P: Provider>(provider: P, tools: ToolRegistry) -> NeuronTurn<P> {
        NeuronTurn::new(
            provider,
            tools,
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            NeuronTurnConfig::default(),
        )
    }

    fn simple_input(text: &str) -> TurnInput {
        TurnInput::new(Content::text(text), layer0::turn::TriggerType::User)
    }

    // ── Tests ──────────────────────────────────────────

    #[tokio::test]
    async fn simple_completion() {
        let provider = MockProvider::new(vec![simple_text_response("Hello!")]);
        let turn = make_turn(provider);

        let output = turn.execute(simple_input("Hi")).await.unwrap();

        assert_eq!(output.exit_reason, ExitReason::Complete);
        assert_eq!(output.message.as_text().unwrap(), "Hello!");
        assert_eq!(output.metadata.tokens_in, 10);
        assert_eq!(output.metadata.tokens_out, 5);
        assert_eq!(output.metadata.turns_used, 1);
        assert!(output.effects.is_empty());
    }

    #[tokio::test]
    async fn tool_use_then_completion() {
        let provider = MockProvider::new(vec![
            tool_use_response("tu_1", "echo", json!({"msg": "test"})),
            simple_text_response("Done!"),
        ]);
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(EchoTool));
        let turn = make_turn_with_tools(provider, tools);

        let output = turn.execute(simple_input("Use echo")).await.unwrap();

        assert_eq!(output.exit_reason, ExitReason::Complete);
        assert_eq!(output.message.as_text().unwrap(), "Done!");
        assert_eq!(output.metadata.turns_used, 2);
        assert_eq!(output.metadata.tools_called.len(), 1);
        assert_eq!(output.metadata.tools_called[0].name, "echo");
        assert!(output.metadata.tools_called[0].success);
    }

    #[tokio::test]
    async fn tool_not_found_backfills_error() {
        let provider = MockProvider::new(vec![
            tool_use_response("tu_1", "nonexistent", json!({})),
            simple_text_response("OK"),
        ]);
        let turn = make_turn(provider);

        let output = turn.execute(simple_input("call tool")).await.unwrap();

        assert_eq!(output.exit_reason, ExitReason::Complete);
        assert_eq!(output.metadata.tools_called.len(), 1);
        assert!(!output.metadata.tools_called[0].success);
    }

    #[tokio::test]
    async fn effect_tool_produces_effect() {
        let provider = MockProvider::new(vec![
            tool_use_response(
                "tu_1",
                "write_memory",
                json!({"scope": "global", "key": "note", "value": "important"}),
            ),
            simple_text_response("Saved!"),
        ]);
        let turn = make_turn(provider);

        let output = turn.execute(simple_input("remember this")).await.unwrap();

        assert_eq!(output.exit_reason, ExitReason::Complete);
        assert_eq!(output.effects.len(), 1);
        match &output.effects[0] {
            Effect::WriteMemory { scope, key, value } => {
                assert_eq!(*scope, Scope::Global);
                assert_eq!(key, "note");
                assert_eq!(*value, json!("important"));
            }
            _ => panic!("expected WriteMemory effect"),
        }
    }

    #[tokio::test]
    async fn max_turns_enforced() {
        // Provider always returns tool use — loop should exit at max_turns
        let responses: Vec<ProviderResponse> = (0..5)
            .map(|i| tool_use_response(&format!("tu_{}", i), "echo", json!({})))
            .collect();
        let provider = MockProvider::new(responses);
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(EchoTool));

        let mut config = NeuronTurnConfig::default();
        config.default_max_turns = 3;

        let turn = NeuronTurn::new(
            provider,
            tools,
            Box::new(NoCompaction),
            HookRegistry::new(),
            Arc::new(NullStateReader),
            config,
        );

        let output = turn.execute(simple_input("loop")).await.unwrap();
        assert_eq!(output.exit_reason, ExitReason::MaxTurns);
        assert_eq!(output.metadata.turns_used, 3);
    }

    #[tokio::test]
    async fn max_tokens_returns_model_error() {
        let provider = MockProvider::new(vec![ProviderResponse {
            content: vec![ContentPart::Text {
                text: "partial...".into(),
            }],
            stop_reason: StopReason::MaxTokens,
            usage: TokenUsage::default(),
            model: "mock".into(),
            cost: None,
            truncated: None,
        }]);
        let turn = make_turn(provider);

        let result = turn.execute(simple_input("Hi")).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TurnError::Model(msg) => assert!(msg.contains("max_tokens")),
            other => panic!("expected TurnError::Model, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn content_filter_returns_model_error() {
        let provider = MockProvider::new(vec![ProviderResponse {
            content: vec![],
            stop_reason: StopReason::ContentFilter,
            usage: TokenUsage::default(),
            model: "mock".into(),
            cost: None,
            truncated: None,
        }]);
        let turn = make_turn(provider);

        let result = turn.execute(simple_input("Hi")).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            TurnError::Model(msg) => assert!(msg.contains("content filtered")),
            other => panic!("expected TurnError::Model, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn cost_aggregated_across_turns() {
        let provider = MockProvider::new(vec![
            tool_use_response("tu_1", "echo", json!({})),
            simple_text_response("Done"),
        ]);
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(EchoTool));
        let turn = make_turn_with_tools(provider, tools);

        let output = turn.execute(simple_input("Hi")).await.unwrap();

        // First call: $0.0002, second call: $0.0001
        assert_eq!(output.metadata.cost, Decimal::new(3, 4));
        assert_eq!(output.metadata.tokens_in, 20);
        assert_eq!(output.metadata.tokens_out, 20);
    }

    #[tokio::test]
    async fn turn_config_overrides_defaults() {
        let provider = MockProvider::new(vec![simple_text_response("Hi")]);
        let turn = make_turn(provider);

        let mut input = simple_input("test");
        input.config = Some(TurnConfig {
            system_addendum: Some("Be concise.".into()),
            model: Some("custom-model".into()),
            max_turns: Some(5),
            ..Default::default()
        });

        let output = turn.execute(input).await.unwrap();
        assert_eq!(output.exit_reason, ExitReason::Complete);
    }

    #[test]
    fn parse_scope_variants() {
        assert_eq!(parse_scope("global"), Scope::Global);
        assert_eq!(
            parse_scope("session:abc"),
            Scope::Session(layer0::SessionId::new("abc"))
        );
        assert_eq!(
            parse_scope("workflow:wf1"),
            Scope::Workflow(layer0::WorkflowId::new("wf1"))
        );
        match parse_scope("other") {
            Scope::Custom(s) => assert_eq!(s, "other"),
            _ => panic!("expected Custom"),
        }
    }
}
```

**Step 2: Update src/lib.rs to include turn module and re-exports**

Full replacement of `neuron-turn/src/lib.rs`:

```rust
#![deny(missing_docs)]
//! ReAct loop implementing `layer0::Turn`.
//!
//! This crate provides [`NeuronTurn`], a full-featured implementation of
//! the [`layer0::Turn`] trait. It runs a ReAct loop: call the model,
//! execute tools, repeat until done.
//!
//! Key traits defined here:
//! - [`Provider`] — LLM provider interface (not object-safe, uses RPITIT)
//! - [`ContextStrategy`] — context window management

pub mod config;
pub mod context;
pub mod convert;
pub mod provider;
pub mod turn;
pub mod types;

// Re-exports
pub use config::NeuronTurnConfig;
pub use context::{ContextStrategy, NoCompaction};
pub use convert::{
    content_block_to_part, content_part_to_block, content_to_parts, content_to_user_message,
    parts_to_content,
};
pub use provider::{Provider, ProviderError};
pub use turn::NeuronTurn;
pub use types::*;
```

**Step 3: Build and test**

```bash
cargo test -p neuron-turn && cargo clippy -p neuron-turn -- -D warnings
```

Expected: All tests pass (types, provider, context, convert, turn modules).

**Step 4: Commit**

```bash
git add neuron-turn/src/
git commit -m "feat(neuron-turn): NeuronTurn struct with full ReAct loop implementing layer0::Turn"
```

---

## Task 7: neuron-context — SlidingWindow Strategy

**Files:**
- Create: `neuron-context/Cargo.toml`
- Create: `neuron-context/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "neuron-context"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "Context strategy implementations for neuron-turn"

[dependencies]
neuron-turn = { path = "../neuron-turn" }
```

**Step 2: Create src/lib.rs**

```rust
#![deny(missing_docs)]
//! Context strategy implementations for neuron-turn.
//!
//! Provides [`SlidingWindow`] for dropping oldest messages when context
//! exceeds a limit. [`NoCompaction`] is in neuron-turn itself.

use neuron_turn::context::ContextStrategy;
use neuron_turn::types::{ContentPart, ProviderMessage};

/// Sliding window context strategy.
///
/// When context exceeds the limit, drops the oldest messages
/// (keeping the first message, which is typically the initial user message).
pub struct SlidingWindow {
    /// Approximate chars-per-token ratio for estimation.
    chars_per_token: usize,
}

impl SlidingWindow {
    /// Create a new sliding window strategy.
    ///
    /// `chars_per_token` controls the token estimation granularity
    /// (default: 4 chars per token).
    pub fn new() -> Self {
        Self { chars_per_token: 4 }
    }

    /// Create with a custom chars-per-token ratio.
    pub fn with_ratio(chars_per_token: usize) -> Self {
        Self {
            chars_per_token: chars_per_token.max(1),
        }
    }

    fn estimate_message_tokens(&self, msg: &ProviderMessage) -> usize {
        msg.content
            .iter()
            .map(|part| match part {
                ContentPart::Text { text } => text.len() / self.chars_per_token,
                ContentPart::ToolUse { input, .. } => input.to_string().len() / self.chars_per_token,
                ContentPart::ToolResult { content, .. } => content.len() / self.chars_per_token,
                ContentPart::Image { .. } => 1000,
            })
            .sum::<usize>()
            + 4 // overhead per message (role, formatting)
    }
}

impl Default for SlidingWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextStrategy for SlidingWindow {
    fn token_estimate(&self, messages: &[ProviderMessage]) -> usize {
        messages
            .iter()
            .map(|m| self.estimate_message_tokens(m))
            .sum()
    }

    fn should_compact(&self, messages: &[ProviderMessage], limit: usize) -> bool {
        self.token_estimate(messages) > limit
    }

    fn compact(&self, messages: Vec<ProviderMessage>) -> Vec<ProviderMessage> {
        if messages.len() <= 2 {
            return messages;
        }

        // Keep first message + most recent messages that fit
        let first = messages[0].clone();
        let rest = &messages[1..];

        // Work backwards, accumulating messages until we hit roughly half the
        // original size (heuristic: keep recent context, drop old)
        let total_tokens: usize = messages.iter().map(|m| self.estimate_message_tokens(m)).sum();
        let target = total_tokens / 2;

        let mut kept = Vec::new();
        let mut current_tokens = self.estimate_message_tokens(&first);

        for msg in rest.iter().rev() {
            let msg_tokens = self.estimate_message_tokens(msg);
            if current_tokens + msg_tokens > target && !kept.is_empty() {
                break;
            }
            kept.push(msg.clone());
            current_tokens += msg_tokens;
        }

        kept.reverse();
        let mut result = vec![first];
        result.extend(kept);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neuron_turn::types::Role;

    fn text_message(role: Role, text: &str) -> ProviderMessage {
        ProviderMessage {
            role,
            content: vec![ContentPart::Text {
                text: text.to_string(),
            }],
        }
    }

    #[test]
    fn sliding_window_estimates_tokens() {
        let sw = SlidingWindow::new();
        let messages = vec![text_message(Role::User, &"a".repeat(400))];
        // 400 chars / 4 = 100, + 4 overhead = 104
        assert_eq!(sw.token_estimate(&messages), 104);
    }

    #[test]
    fn sliding_window_should_compact() {
        let sw = SlidingWindow::new();
        let messages = vec![text_message(Role::User, &"a".repeat(400))];
        assert!(sw.should_compact(&messages, 50));
        assert!(!sw.should_compact(&messages, 200));
    }

    #[test]
    fn sliding_window_compact_preserves_first_and_recent() {
        let sw = SlidingWindow::new();
        let messages = vec![
            text_message(Role::User, &"first ".repeat(100)),
            text_message(Role::Assistant, &"old ".repeat(100)),
            text_message(Role::User, &"middle ".repeat(100)),
            text_message(Role::Assistant, &"recent ".repeat(100)),
            text_message(Role::User, &"latest ".repeat(100)),
        ];

        let compacted = sw.compact(messages.clone());

        // Should keep first message
        assert_eq!(compacted[0].role, Role::User);
        assert!(compacted[0].content[0] == messages[0].content[0]);

        // Should keep some recent messages
        assert!(compacted.len() < messages.len());
        assert!(compacted.len() >= 2);

        // Last message should be the latest
        assert_eq!(
            compacted.last().unwrap().content[0],
            messages.last().unwrap().content[0]
        );
    }

    #[test]
    fn sliding_window_short_messages_unchanged() {
        let sw = SlidingWindow::new();
        let messages = vec![
            text_message(Role::User, "hi"),
            text_message(Role::Assistant, "hello"),
        ];

        let compacted = sw.compact(messages.clone());
        assert_eq!(compacted.len(), messages.len());
    }

    #[test]
    fn sliding_window_single_message_unchanged() {
        let sw = SlidingWindow::new();
        let messages = vec![text_message(Role::User, "hi")];
        let compacted = sw.compact(messages.clone());
        assert_eq!(compacted.len(), 1);
    }
}
```

**Step 3: Add neuron-context to workspace**

In `Cargo.toml` (workspace root), add `"neuron-context"` to the members list.

**Step 4: Build and test**

```bash
cargo build && cargo test -p neuron-context && cargo clippy -p neuron-context -- -D warnings
```

**Step 5: Commit**

```bash
git add neuron-context/ Cargo.toml
git commit -m "feat(neuron-context): add SlidingWindow context strategy"
```

---

## Task 8: neuron-provider-anthropic — Anthropic Provider

**Files:**
- Create: `neuron-provider-anthropic/Cargo.toml`
- Create: `neuron-provider-anthropic/src/lib.rs`
- Create: `neuron-provider-anthropic/src/types.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "neuron-provider-anthropic"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "Anthropic API provider for neuron-turn"

[dependencies]
neuron-turn = { path = "../neuron-turn" }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rust_decimal = { version = "1", features = ["serde-str"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
layer0 = { path = "../layer0" }
neuron-tool = { path = "../neuron-tool" }
neuron-hooks = { path = "../neuron-hooks" }
neuron-context = { path = "../neuron-context" }
neuron-state-memory = { path = "../neuron-state-memory" }
```

**Step 2: Create src/types.rs (Anthropic API serde types)**

```rust
//! Anthropic Messages API request/response types.

use serde::{Deserialize, Serialize};

/// Anthropic API request body.
#[derive(Debug, Serialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<AnthropicTool>,
}

/// A message in the Anthropic API format.
#[derive(Debug, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: AnthropicContent,
}

/// Content can be a string or array of content blocks.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnthropicContent {
    /// Simple text string.
    Text(String),
    /// Array of content blocks.
    Blocks(Vec<AnthropicContentBlock>),
}

/// A content block in the Anthropic API format.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnthropicContentBlock {
    /// Text content.
    #[serde(rename = "text")]
    Text { text: String },
    /// Tool use request.
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result.
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
    /// Image content.
    #[serde(rename = "image")]
    Image {
        source: AnthropicImageSource,
        media_type: String,
    },
}

/// Image source in Anthropic API format.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AnthropicImageSource {
    #[serde(rename = "base64")]
    Base64 { data: String },
    #[serde(rename = "url")]
    Url { url: String },
}

/// Tool definition for the Anthropic API.
#[derive(Debug, Serialize)]
pub struct AnthropicTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Anthropic API response body.
#[derive(Debug, Deserialize)]
pub struct AnthropicResponse {
    pub content: Vec<AnthropicContentBlock>,
    pub model: String,
    pub stop_reason: String,
    pub usage: AnthropicUsage,
}

/// Token usage from the Anthropic API.
#[derive(Debug, Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
}

/// Anthropic API error response.
#[derive(Debug, Deserialize)]
pub struct AnthropicError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub error: AnthropicErrorDetail,
}

/// Error detail from the Anthropic API.
#[derive(Debug, Deserialize)]
pub struct AnthropicErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}
```

**Step 3: Create src/lib.rs (AnthropicProvider + conversions)**

```rust
#![deny(missing_docs)]
//! Anthropic API provider for neuron-turn.
//!
//! Implements the [`neuron_turn::Provider`] trait for Anthropic's Messages API.

mod types;

use neuron_turn::provider::{Provider, ProviderError};
use neuron_turn::types::*;
use rust_decimal::Decimal;
use types::*;

/// Anthropic API provider.
pub struct AnthropicProvider {
    api_key: String,
    client: reqwest::Client,
    api_url: String,
    api_version: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: reqwest::Client::new(),
            api_url: "https://api.anthropic.com/v1/messages".into(),
            api_version: "2023-06-01".into(),
        }
    }

    /// Override the API URL (for testing or proxies).
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.api_url = url.into();
        self
    }

    fn build_request(&self, request: &ProviderRequest) -> AnthropicRequest {
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| "claude-haiku-4-5-20251001".into());
        let max_tokens = request.max_tokens.unwrap_or(4096);

        let messages: Vec<AnthropicMessage> = request
            .messages
            .iter()
            .map(|m| AnthropicMessage {
                role: match m.role {
                    Role::User => "user".into(),
                    Role::Assistant => "assistant".into(),
                    Role::System => "user".into(), // System messages go in the system field
                },
                content: parts_to_anthropic_content(&m.content),
            })
            .collect();

        let tools: Vec<AnthropicTool> = request
            .tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.input_schema.clone(),
            })
            .collect();

        AnthropicRequest {
            model,
            max_tokens,
            messages,
            system: request.system.clone(),
            tools,
        }
    }

    fn parse_response(&self, response: AnthropicResponse) -> ProviderResponse {
        let content: Vec<ContentPart> = response
            .content
            .iter()
            .map(anthropic_block_to_content_part)
            .collect();

        let stop_reason = match response.stop_reason.as_str() {
            "end_turn" => StopReason::EndTurn,
            "tool_use" => StopReason::ToolUse,
            "max_tokens" => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        };

        let usage = TokenUsage {
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            cache_read_tokens: response.usage.cache_read_input_tokens,
            cache_creation_tokens: response.usage.cache_creation_input_tokens,
        };

        // Simple cost calculation for Haiku
        // Haiku: $0.25/MTok input, $1.25/MTok output (as of 2025)
        let input_cost =
            Decimal::from(response.usage.input_tokens) * Decimal::new(25, 8);
        let output_cost =
            Decimal::from(response.usage.output_tokens) * Decimal::new(125, 8);
        let cost = input_cost + output_cost;

        ProviderResponse {
            content,
            stop_reason,
            usage,
            model: response.model,
            cost: Some(cost),
            truncated: None,
        }
    }
}

impl Provider for AnthropicProvider {
    fn complete(
        &self,
        request: ProviderRequest,
    ) -> impl std::future::Future<Output = Result<ProviderResponse, ProviderError>> + Send {
        let api_request = self.build_request(&request);
        let http_request = self
            .client
            .post(&self.api_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json(&api_request);

        async move {
            let http_response = http_request
                .send()
                .await
                .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

            let status = http_response.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(ProviderError::RateLimited);
            }
            if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
                let body = http_response.text().await.unwrap_or_default();
                return Err(ProviderError::AuthFailed(body));
            }
            if !status.is_success() {
                let body = http_response.text().await.unwrap_or_default();
                return Err(ProviderError::RequestFailed(format!(
                    "HTTP {}: {}",
                    status, body
                )));
            }

            let api_response: AnthropicResponse = http_response
                .json()
                .await
                .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

            Ok(self.parse_response(api_response))
        }
    }
}

fn parts_to_anthropic_content(parts: &[ContentPart]) -> AnthropicContent {
    if parts.len() == 1 {
        if let ContentPart::Text { text } = &parts[0] {
            return AnthropicContent::Text(text.clone());
        }
    }
    AnthropicContent::Blocks(parts.iter().map(content_part_to_anthropic_block).collect())
}

fn content_part_to_anthropic_block(part: &ContentPart) -> AnthropicContentBlock {
    match part {
        ContentPart::Text { text } => AnthropicContentBlock::Text { text: text.clone() },
        ContentPart::ToolUse { id, name, input } => AnthropicContentBlock::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        ContentPart::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => AnthropicContentBlock::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: content.clone(),
            is_error: *is_error,
        },
        ContentPart::Image { source, media_type } => AnthropicContentBlock::Image {
            source: match source {
                ImageSource::Base64 { data } => AnthropicImageSource::Base64 { data: data.clone() },
                ImageSource::Url { url } => AnthropicImageSource::Url { url: url.clone() },
            },
            media_type: media_type.clone(),
        },
    }
}

fn anthropic_block_to_content_part(block: &AnthropicContentBlock) -> ContentPart {
    match block {
        AnthropicContentBlock::Text { text } => ContentPart::Text { text: text.clone() },
        AnthropicContentBlock::ToolUse { id, name, input } => ContentPart::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: input.clone(),
        },
        AnthropicContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => ContentPart::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: content.clone(),
            is_error: *is_error,
        },
        AnthropicContentBlock::Image { source, media_type } => ContentPart::Image {
            source: match source {
                AnthropicImageSource::Base64 { data } => ImageSource::Base64 { data: data.clone() },
                AnthropicImageSource::Url { url } => ImageSource::Url { url: url.clone() },
            },
            media_type: media_type.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_simple_request() {
        let provider = AnthropicProvider::new("test-key");
        let request = ProviderRequest {
            model: Some("claude-haiku-4-5-20251001".into()),
            messages: vec![ProviderMessage {
                role: Role::User,
                content: vec![ContentPart::Text {
                    text: "Hello".into(),
                }],
            }],
            tools: vec![],
            max_tokens: Some(256),
            temperature: None,
            system: Some("Be helpful.".into()),
            extra: json!(null),
        };

        let api_request = provider.build_request(&request);
        assert_eq!(api_request.model, "claude-haiku-4-5-20251001");
        assert_eq!(api_request.max_tokens, 256);
        assert_eq!(api_request.messages.len(), 1);
        assert_eq!(api_request.messages[0].role, "user");
        assert_eq!(api_request.system, Some("Be helpful.".into()));
    }

    #[test]
    fn parse_simple_response() {
        let provider = AnthropicProvider::new("test-key");
        let api_response = AnthropicResponse {
            content: vec![AnthropicContentBlock::Text {
                text: "Hello!".into(),
            }],
            model: "claude-haiku-4-5-20251001".into(),
            stop_reason: "end_turn".into(),
            usage: AnthropicUsage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
        };

        let response = provider.parse_response(api_response);
        assert_eq!(response.stop_reason, StopReason::EndTurn);
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
        assert!(response.cost.is_some());
        assert_eq!(response.content.len(), 1);
    }

    #[test]
    fn parse_tool_use_response() {
        let provider = AnthropicProvider::new("test-key");
        let api_response = AnthropicResponse {
            content: vec![AnthropicContentBlock::ToolUse {
                id: "tu_1".into(),
                name: "bash".into(),
                input: json!({"command": "ls"}),
            }],
            model: "claude-haiku-4-5-20251001".into(),
            stop_reason: "tool_use".into(),
            usage: AnthropicUsage {
                input_tokens: 20,
                output_tokens: 30,
                cache_read_input_tokens: None,
                cache_creation_input_tokens: None,
            },
        };

        let response = provider.parse_response(api_response);
        assert_eq!(response.stop_reason, StopReason::ToolUse);
        assert_eq!(response.content.len(), 1);
        match &response.content[0] {
            ContentPart::ToolUse { name, .. } => assert_eq!(name, "bash"),
            _ => panic!("expected ToolUse"),
        }
    }

    #[test]
    fn tool_schema_serializes() {
        let tool = AnthropicTool {
            name: "get_weather".into(),
            description: "Get current weather".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "location": {"type": "string"}
                },
                "required": ["location"]
            }),
        };
        let json = serde_json::to_value(&tool).unwrap();
        assert_eq!(json["name"], "get_weather");
    }
}
```

**Step 4: Add neuron-provider-anthropic to workspace**

In `Cargo.toml` (workspace root), add `"neuron-provider-anthropic"` to the members list.

**Step 5: Build and test**

```bash
cargo build && cargo test -p neuron-provider-anthropic && cargo clippy -p neuron-provider-anthropic -- -D warnings
```

**Step 6: Commit**

```bash
git add neuron-provider-anthropic/ Cargo.toml
git commit -m "feat(neuron-provider-anthropic): Anthropic Messages API provider"
```

---

## Task 9: Integration Test — Real Anthropic Haiku Call

**Files:**
- Create: `neuron-provider-anthropic/tests/integration.rs`

**Step 1: Create the integration test**

```rust
//! Integration test: real Anthropic Haiku call through the full stack.

use layer0::content::Content;
use layer0::turn::{ExitReason, Turn, TurnInput, TriggerType};
use neuron_context::SlidingWindow;
use neuron_hooks::HookRegistry;
use neuron_provider_anthropic::AnthropicProvider;
use neuron_tool::ToolRegistry;
use neuron_turn::{NeuronTurn, NeuronTurnConfig};
use std::sync::Arc;

#[tokio::test]
#[ignore] // Requires ANTHROPIC_API_KEY environment variable
async fn real_haiku_simple_completion() {
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");

    let provider = AnthropicProvider::new(api_key);
    let tools = ToolRegistry::new();
    let strategy = Box::new(SlidingWindow::new());
    let hooks = HookRegistry::new();
    let store = Arc::new(neuron_state_memory::MemoryStore::new()) as Arc<dyn layer0::StateReader>;

    let config = NeuronTurnConfig {
        system_prompt: "You are a helpful assistant. Be very concise.".into(),
        default_model: "claude-haiku-4-5-20251001".into(),
        default_max_tokens: 128,
        default_max_turns: 5,
    };

    let turn = NeuronTurn::new(provider, tools, strategy, hooks, store, config);

    let input = TurnInput::new(
        Content::text("Say hello in exactly 3 words."),
        TriggerType::User,
    );

    let output = turn.execute(input).await.unwrap();

    assert_eq!(output.exit_reason, ExitReason::Complete);
    assert!(output.message.as_text().is_some());
    let text = output.message.as_text().unwrap();
    assert!(!text.is_empty());
    assert!(output.metadata.tokens_in > 0);
    assert!(output.metadata.tokens_out > 0);
    assert!(output.metadata.cost > rust_decimal::Decimal::ZERO);
    assert_eq!(output.metadata.turns_used, 1);
    assert!(output.effects.is_empty());
}

#[tokio::test]
#[ignore] // Requires ANTHROPIC_API_KEY environment variable
async fn neuron_turn_is_object_safe_as_arc_dyn_turn() {
    let api_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY not set");

    let provider = AnthropicProvider::new(api_key);
    let tools = ToolRegistry::new();
    let strategy = Box::new(SlidingWindow::new());
    let hooks = HookRegistry::new();
    let store = Arc::new(neuron_state_memory::MemoryStore::new()) as Arc<dyn layer0::StateReader>;

    let config = NeuronTurnConfig {
        system_prompt: "You are a helpful assistant.".into(),
        default_model: "claude-haiku-4-5-20251001".into(),
        default_max_tokens: 64,
        default_max_turns: 3,
    };

    // Prove NeuronTurn<P> can be used as Arc<dyn Turn>
    let turn: Arc<dyn Turn> = Arc::new(NeuronTurn::new(
        provider, tools, strategy, hooks, store, config,
    ));

    let input = TurnInput::new(Content::text("Say hi."), TriggerType::User);
    let output = turn.execute(input).await.unwrap();
    assert_eq!(output.exit_reason, ExitReason::Complete);
}
```

**Step 2: Build (don't run ignored tests)**

```bash
cargo build && cargo test -p neuron-provider-anthropic && cargo clippy -p neuron-provider-anthropic -- -D warnings
```

**Step 3: Run integration test (requires API key)**

```bash
ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY" cargo test -p neuron-provider-anthropic --test integration -- --ignored
```

Expected: Both tests pass (real HTTP calls to Anthropic).

**Step 4: Commit**

```bash
git add neuron-provider-anthropic/tests/
git commit -m "test: integration test with real Anthropic Haiku calls"
```

---

## Task 10: Final Verification

**Step 1: Run the full check suite across all crates**

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

Expected: All tests pass across all crates, zero warnings, clean docs.

**Step 2: Run integration tests if API key is available**

```bash
ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY" cargo test -p neuron-provider-anthropic --test integration -- --ignored
```

**Step 3: Verify test count**

```bash
cargo test 2>&1 | grep "test result"
```

Expected: 165 existing + ~40 new = ~205+ tests passing.

---

## Verification Checklist

After all tasks, verify:

- [ ] `cargo build` — all 10 workspace crates compile
- [ ] `cargo test` — all tests pass (unit + integration minus ignored)
- [ ] `cargo clippy -- -D warnings` — zero warnings
- [ ] `cargo doc --no-deps` — docs build clean
- [ ] `NeuronTurn<P>` implements `layer0::Turn` (object-safe via async_trait)
- [ ] Provider trait uses RPITIT (not object-safe, by design)
- [ ] ContextStrategy is swappable (NoCompaction, SlidingWindow)
- [ ] ToolDyn is object-safe (Arc<dyn ToolDyn>)
- [ ] Effect tools produce layer0::Effect variants
- [ ] Real Anthropic Haiku call succeeds (integration test)
- [ ] No layer0 changes required
