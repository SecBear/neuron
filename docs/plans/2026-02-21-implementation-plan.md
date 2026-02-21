# rust-agent-blocks Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement all 10 crates of rust-agent-blocks from the design doc, in dependency order, with TDD and frequent commits.

**Architecture:** Independent Rust crates composing via traits defined in `agent-types`. Each crate is its own Cargo project with path dependencies during development. Flat file layout per crate. Arrows only point up — no circular dependencies.

**Tech Stack:** Rust 2024 edition (resolver 3, min 1.90), serde/serde_json, thiserror, schemars, futures, tokio, reqwest, rmcp, tracing, chrono, uuid, tokio-util

**Design Spec:** `docs/plans/2026-02-21-rust-agent-blocks-design.md` — every type, trait, and error enum is defined there. This plan translates that spec into ordered coding tasks.

---

## Block 1: `agent-types` (Foundation)

Everything depends on this. Zero logic — pure types, traits, serde.

### Task 1.1: Initialize the Cargo project

**Files:**
- Create: `agent-types/Cargo.toml`
- Create: `agent-types/src/lib.rs`
- Create: `agent-types/CLAUDE.md`

**Step 1: Create project structure**

```bash
mkdir -p agent-types/src agent-types/tests
```

**Step 2: Write Cargo.toml**

```toml
[package]
name = "agent-types"
version = "0.1.0"
edition = "2024"
rust-version = "1.90"
description = "Shared types and traits for rust-agent-blocks"
license = "MIT OR Apache-2.0"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
schemars = "0.8"
futures = "0.3"
tokio-util = "0.7"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

**Step 3: Write minimal lib.rs that compiles**

```rust
//! Shared types and traits for rust-agent-blocks.
//!
//! This crate defines the lingua franca — messages, providers, tools, errors —
//! that all other agent-blocks crates depend on. Zero logic, pure types.

pub mod types;
pub mod traits;
pub mod error;
pub mod wasm;
pub mod stream;
```

**Step 4: Create empty module files**

Create `src/types.rs`, `src/traits.rs`, `src/error.rs`, `src/wasm.rs`, `src/stream.rs` — each with a comment placeholder.

**Step 5: Verify it compiles**

Run: `cd agent-types && cargo check`
Expected: success (compiles with empty modules)

**Step 6: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): initialize crate with project structure"
```

---

### Task 1.2: WASM compatibility types

**Files:**
- Modify: `agent-types/src/wasm.rs`
- Test: `agent-types/src/wasm.rs` (inline tests)

**Step 1: Write failing test**

In `src/wasm.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;

    fn assert_wasm_compat_send<T: WasmCompatSend>() {}
    fn assert_wasm_compat_sync<T: WasmCompatSync>() {}

    #[test]
    fn string_is_wasm_compat_send() {
        assert_wasm_compat_send::<String>();
    }

    #[test]
    fn string_is_wasm_compat_sync() {
        assert_wasm_compat_sync::<String>();
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cd agent-types && cargo test wasm`
Expected: FAIL — `WasmCompatSend` not found

**Step 3: Implement WASM compat types**

```rust
//! WASM compatibility shims.
//!
//! On native targets, these are aliases for Send/Sync.
//! On wasm32, the bounds are removed since wasm32 is single-threaded.

use std::future::Future;
use std::pin::Pin;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;

    pub trait WasmCompatSend: Send {}
    impl<T: Send> WasmCompatSend for T {}

    pub trait WasmCompatSync: Sync {}
    impl<T: Sync> WasmCompatSync for T {}

    pub type WasmBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;

    pub trait WasmCompatSend {}
    impl<T> WasmCompatSend for T {}

    pub trait WasmCompatSync {}
    impl<T> WasmCompatSync for T {}

    pub type WasmBoxedFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
```

**Step 4: Run test to verify it passes**

Run: `cd agent-types && cargo test wasm`
Expected: PASS

**Step 5: Commit**

```bash
git add agent-types/src/wasm.rs
git commit -m "feat(agent-types): add WASM compatibility traits"
```

---

### Task 1.3: Message types

**Files:**
- Modify: `agent-types/src/types.rs`
- Test: `agent-types/tests/messages.rs`

**Step 1: Write failing test**

Create `agent-types/tests/messages.rs`:

```rust
use agent_types::*;

#[test]
fn message_roundtrip_serde() {
    let msg = Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Text("hello".into()),
            ContentBlock::ToolUse {
                id: "t1".into(),
                name: "read_file".into(),
                input: serde_json::json!({"path": "/tmp/foo"}),
            },
        ],
    };
    let json = serde_json::to_string(&msg).unwrap();
    let roundtrip: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.role, Role::Assistant);
    assert_eq!(roundtrip.content.len(), 2);
}

#[test]
fn tool_result_with_content_items() {
    let block = ContentBlock::ToolResult {
        tool_use_id: "t1".into(),
        content: vec![
            ContentItem::Text("file contents here".into()),
            ContentItem::Image {
                source: ImageSource::Url { url: "https://example.com/img.png".into() },
            },
        ],
        is_error: false,
    };
    let json = serde_json::to_string(&block).unwrap();
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::ToolResult { content, is_error, .. } = rt {
        assert_eq!(content.len(), 2);
        assert!(!is_error);
    } else {
        panic!("expected ToolResult");
    }
}

#[test]
fn thinking_block_serde() {
    let block = ContentBlock::Thinking {
        thinking: "Let me consider...".into(),
        signature: "sig123".into(),
    };
    let json = serde_json::to_string(&block).unwrap();
    assert!(json.contains("thinking"));
    let rt: ContentBlock = serde_json::from_str(&json).unwrap();
    if let ContentBlock::Thinking { thinking, .. } = rt {
        assert_eq!(thinking, "Let me consider...");
    } else {
        panic!("expected Thinking");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cd agent-types && cargo test --test messages`
Expected: FAIL — types not defined

**Step 3: Implement message types in `src/types.rs`**

Implement `Role`, `ContentBlock`, `ContentItem`, `ImageSource`, `DocumentSource`, `Message` exactly as specified in design doc section 4.1 "Messages". All derive `Debug, Clone, Serialize, Deserialize`. Add `PartialEq` on `Role`.

Re-export from `lib.rs`:
```rust
pub use types::*;
```

**Step 4: Run test to verify it passes**

Run: `cd agent-types && cargo test --test messages`
Expected: PASS

**Step 5: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): add message types with serde"
```

---

### Task 1.4: CompletionRequest and response types

**Files:**
- Modify: `agent-types/src/types.rs`
- Test: `agent-types/tests/completion.rs`

**Step 1: Write failing test**

Create `agent-types/tests/completion.rs`:

```rust
use agent_types::*;

#[test]
fn completion_request_minimal() {
    let req = CompletionRequest {
        model: "claude-sonnet-4-20250514".into(),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("hi".into())],
        }],
        system: None,
        tools: vec![],
        max_tokens: Some(1024),
        temperature: None,
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("claude-sonnet"));
}

#[test]
fn system_prompt_blocks_with_cache() {
    let sys = SystemPrompt::Blocks(vec![
        SystemBlock {
            text: "You are helpful.".into(),
            cache_control: Some(CacheControl {
                ttl: Some(CacheTtl::OneHour),
            }),
        },
    ]);
    let json = serde_json::to_string(&sys).unwrap();
    let rt: SystemPrompt = serde_json::from_str(&json).unwrap();
    if let SystemPrompt::Blocks(blocks) = rt {
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].cache_control.is_some());
    } else {
        panic!("expected Blocks");
    }
}

#[test]
fn completion_response_serde() {
    let resp = CompletionResponse {
        id: "msg_123".into(),
        model: "claude-sonnet-4-20250514".into(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text("Paris".into())],
        },
        usage: TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: None,
            cache_creation_tokens: None,
            reasoning_tokens: None,
        },
        stop_reason: StopReason::EndTurn,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let rt: CompletionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.id, "msg_123");
}

#[test]
fn token_usage_default() {
    let usage = TokenUsage::default();
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 0);
}
```

**Step 2: Run test to verify it fails**

Run: `cd agent-types && cargo test --test completion`
Expected: FAIL — types not defined

**Step 3: Implement all completion-related types**

In `src/types.rs`, add: `SystemPrompt`, `SystemBlock`, `CacheControl`, `CacheTtl`, `ToolChoice`, `ResponseFormat`, `ThinkingConfig`, `ReasoningEffort`, `CompletionRequest`, `CompletionResponse`, `StopReason`, `TokenUsage` — all exactly as specified in design doc section 4.1.

**Step 4: Run test to verify it passes**

Run: `cd agent-types && cargo test --test completion`
Expected: PASS

**Step 5: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): add completion request/response types"
```

---

### Task 1.5: Tool-related types

**Files:**
- Modify: `agent-types/src/types.rs`
- Test: `agent-types/tests/tool_types.rs`

**Step 1: Write failing test**

Create `agent-types/tests/tool_types.rs`:

```rust
use agent_types::*;

#[test]
fn tool_definition_serde() {
    let def = ToolDefinition {
        name: "read_file".into(),
        title: None,
        description: "Read a file".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        }),
        output_schema: None,
        annotations: Some(ToolAnnotations {
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: None,
        }),
        cache_control: None,
    };
    let json = serde_json::to_string(&def).unwrap();
    let rt: ToolDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(rt.name, "read_file");
    assert!(rt.annotations.unwrap().read_only_hint.unwrap());
}

#[test]
fn tool_output_text() {
    let output = ToolOutput {
        content: vec![ContentItem::Text("file contents".into())],
        structured_content: None,
        is_error: false,
    };
    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("file contents"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd agent-types && cargo test --test tool_types`
Expected: FAIL

**Step 3: Implement tool types**

In `src/types.rs`, add: `ToolDefinition`, `ToolAnnotations`, `ToolOutput`, `ToolContext`, `ProgressReporter` trait — as specified in design doc section 4.1 "Tool trait".

Note: `ToolContext` has non-serde fields (`CancellationToken`, `Arc<dyn ProgressReporter>`), so it should NOT derive Serialize/Deserialize. Only the data transfer types do.

**Step 4: Run tests**

Run: `cd agent-types && cargo test --test tool_types`
Expected: PASS

**Step 5: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): add tool-related types"
```

---

### Task 1.6: Error types

**Files:**
- Modify: `agent-types/src/error.rs`
- Test: `agent-types/tests/errors.rs`

**Step 1: Write failing test**

Create `agent-types/tests/errors.rs`:

```rust
use agent_types::*;
use std::time::Duration;

#[test]
fn provider_error_display() {
    let err = ProviderError::RateLimit { retry_after: Some(Duration::from_secs(30)) };
    assert!(err.to_string().contains("rate limited"));
}

#[test]
fn provider_error_is_retryable() {
    assert!(ProviderError::Network(
        Box::new(std::io::Error::new(std::io::ErrorKind::Other, "timeout"))
    ).is_retryable());
    assert!(!ProviderError::Authentication("bad key".into()).is_retryable());
}

#[test]
fn loop_error_from_provider() {
    let pe = ProviderError::Authentication("bad".into());
    let le: LoopError = pe.into();
    assert!(le.to_string().contains("provider error"));
}

#[test]
fn tool_error_display() {
    let err = ToolError::NotFound("read_file".into());
    assert!(err.to_string().contains("read_file"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd agent-types && cargo test --test errors`
Expected: FAIL

**Step 3: Implement all error types**

In `src/error.rs`, implement: `ProviderError`, `ToolError`, `ContextError`, `LoopError`, `DurableError`, `McpError`, `HookError`, `StorageError`, `SubAgentError`, `SandboxError` — exactly as in design doc section 4.1 "Error types".

Add `is_retryable()` method on `ProviderError`:
```rust
impl ProviderError {
    pub fn is_retryable(&self) -> bool {
        matches!(self,
            Self::Network(_) | Self::RateLimit { .. } |
            Self::ModelLoading(_) | Self::Timeout(_) |
            Self::ServiceUnavailable(_)
        )
    }
}
```

**Step 4: Run tests**

Run: `cd agent-types && cargo test --test errors`
Expected: PASS

**Step 5: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): add error types with retryable classification"
```

---

### Task 1.7: Stream types

**Files:**
- Modify: `agent-types/src/stream.rs`
- Test: `agent-types/tests/stream.rs`

**Step 1: Write failing test**

Create `agent-types/tests/stream.rs`:

```rust
use agent_types::*;

#[test]
fn stream_event_text_delta() {
    let event = StreamEvent::TextDelta("hello".into());
    match event {
        StreamEvent::TextDelta(s) => assert_eq!(s, "hello"),
        _ => panic!("expected TextDelta"),
    }
}

#[test]
fn stream_event_tool_use_demux() {
    let events = vec![
        StreamEvent::ToolUseStart { id: "t1".into(), name: "read_file".into() },
        StreamEvent::ToolUseInputDelta { id: "t1".into(), delta: r#"{"path"#.into() },
        StreamEvent::ToolUseInputDelta { id: "t1".into(), delta: r#": "/tmp"}"#.into() },
        StreamEvent::ToolUseEnd { id: "t1".into() },
    ];
    // Verify we can match on id for parallel tool call demux
    let t1_deltas: Vec<&str> = events.iter().filter_map(|e| match e {
        StreamEvent::ToolUseInputDelta { id, delta } if id == "t1" => Some(delta.as_str()),
        _ => None,
    }).collect();
    assert_eq!(t1_deltas.join(""), r#"{"path": "/tmp"}"#);
}
```

**Step 2: Run test to verify it fails**

Run: `cd agent-types && cargo test --test stream`
Expected: FAIL

**Step 3: Implement stream types**

In `src/stream.rs`, implement `StreamEvent` and `StreamHandle` exactly as in design doc.

**Step 4: Run tests**

Run: `cd agent-types && cargo test --test stream`
Expected: PASS

**Step 5: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): add stream event types"
```

---

### Task 1.8: Provider trait

**Files:**
- Modify: `agent-types/src/traits.rs`
- Test: `agent-types/tests/provider_trait.rs`

**Step 1: Write failing test**

Create `agent-types/tests/provider_trait.rs`:

```rust
use agent_types::*;
use std::future::Future;

/// A mock provider that always returns a fixed response.
struct MockProvider;

impl Provider for MockProvider {
    fn complete(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse, ProviderError>> + Send {
        async {
            Ok(CompletionResponse {
                id: "mock_1".into(),
                model: "mock".into(),
                message: Message {
                    role: Role::Assistant,
                    content: vec![ContentBlock::Text("mock response".into())],
                },
                usage: TokenUsage::default(),
                stop_reason: StopReason::EndTurn,
            })
        }
    }

    fn complete_stream(
        &self,
        _request: CompletionRequest,
    ) -> impl Future<Output = Result<StreamHandle, ProviderError>> + Send {
        async { Err(ProviderError::Other("streaming not implemented".into())) }
    }
}

#[tokio::test]
async fn mock_provider_complete() {
    let provider = MockProvider;
    let request = CompletionRequest {
        model: "mock".into(),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("hi".into())],
        }],
        system: None,
        tools: vec![],
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    };
    let response = provider.complete(request).await.unwrap();
    assert_eq!(response.id, "mock_1");
}

// Verify Provider is object-safe enough for Box<dyn Provider> via our ToolDyn pattern
// Note: Provider itself uses RPITIT so isn't directly object-safe.
// This is fine — we use generics, not trait objects, for Provider.
```

**Step 2: Run test to verify it fails**

Run: `cd agent-types && cargo test --test provider_trait`
Expected: FAIL

**Step 3: Implement Provider trait**

In `src/traits.rs`:

```rust
use crate::*;
use std::future::Future;

/// LLM provider trait. Implement this for each provider (Anthropic, OpenAI, Ollama, etc.).
///
/// Uses RPITIT (return position impl trait in trait) — Rust 2024 native async.
/// Not object-safe by design; use generics `<P: Provider>` to compose.
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
```

**Step 4: Run tests**

Run: `cd agent-types && cargo test --test provider_trait`
Expected: PASS

**Step 5: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): add Provider trait with RPITIT"
```

---

### Task 1.9: Tool and ToolDyn traits with blanket impl

**Files:**
- Modify: `agent-types/src/traits.rs`
- Test: `agent-types/tests/tool_trait.rs`

**Step 1: Write failing test**

Create `agent-types/tests/tool_trait.rs`:

```rust
use agent_types::*;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ReadFileArgs {
    path: String,
}

#[derive(Debug, Serialize)]
struct ReadFileOutput {
    content: String,
}

#[derive(Debug, thiserror::Error)]
enum ReadFileError {
    #[error("file not found: {0}")]
    NotFound(String),
}

struct ReadFileTool;

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";
    type Args = ReadFileArgs;
    type Output = ReadFileOutput;
    type Error = ReadFileError;

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.into(),
            title: None,
            description: "Read a file".into(),
            input_schema: schemars::schema_for!(ReadFileArgs).into(),
            output_schema: None,
            annotations: None,
            cache_control: None,
        }
    }

    fn call(
        &self,
        args: Self::Args,
        _ctx: &ToolContext,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send {
        async move {
            Ok(ReadFileOutput { content: format!("contents of {}", args.path) })
        }
    }
}

#[tokio::test]
async fn tool_dyn_blanket_impl() {
    let tool = ReadFileTool;
    let dyn_tool: &dyn ToolDyn = &tool;

    assert_eq!(dyn_tool.name(), "read_file");

    let ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test".into(),
        environment: HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    };

    let input = serde_json::json!({"path": "/tmp/test.txt"});
    let result = dyn_tool.call_dyn(input, &ctx).await.unwrap();
    assert!(!result.is_error);

    // Verify structured_content round-trips
    let value = result.structured_content.unwrap();
    assert!(value.to_string().contains("contents of /tmp/test.txt"));
}

#[tokio::test]
async fn tool_dyn_invalid_input() {
    let tool = ReadFileTool;
    let dyn_tool: &dyn ToolDyn = &tool;

    let ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "test".into(),
        environment: HashMap::new(),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        progress_reporter: None,
    };

    let input = serde_json::json!({"wrong_field": 42});
    let result = dyn_tool.call_dyn(input, &ctx).await;
    assert!(result.is_err());
}

#[test]
fn schemars_generates_schema() {
    let schema = schemars::schema_for!(ReadFileArgs);
    let json = serde_json::to_value(schema).unwrap();
    // Should have "path" as required property
    let props = json["properties"].as_object().unwrap();
    assert!(props.contains_key("path"));
}
```

**Step 2: Run test to verify it fails**

Run: `cd agent-types && cargo test --test tool_trait`
Expected: FAIL

**Step 3: Implement Tool, ToolDyn, and blanket impl**

In `src/traits.rs`, implement the `Tool` trait, `ToolDyn` trait, and the blanket impl `impl<T: Tool> ToolDyn for T` exactly as specified in design doc section 4.1. The blanket impl:
- Deserializes `serde_json::Value` into `T::Args`
- Calls `T::call(args, ctx)`
- Serializes `T::Output` into `ToolOutput` (both `content` as text and `structured_content` as JSON value)
- Maps `T::Error` into `ToolError::ExecutionFailed`
- Returns `ToolError::InvalidInput` on deserialization failure

Also add the `schemars::schema_for!` helper for converting `JsonSchema` into `serde_json::Value` in `ToolDefinition.input_schema`.

**Step 4: Run tests**

Run: `cd agent-types && cargo test --test tool_trait`
Expected: PASS

**Step 5: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): add Tool/ToolDyn traits with blanket impl"
```

---

### Task 1.10: ContextStrategy, ObservabilityHook, DurableContext, PermissionPolicy traits

**Files:**
- Modify: `agent-types/src/traits.rs`
- Test: `agent-types/tests/traits.rs`

**Step 1: Write failing test**

Create `agent-types/tests/traits.rs`:

```rust
use agent_types::*;
use std::future::Future;

struct NoopHook;

impl ObservabilityHook for NoopHook {
    fn on_event(
        &self,
        _event: HookEvent<'_>,
    ) -> impl Future<Output = Result<HookAction, HookError>> + Send {
        async { Ok(HookAction::Continue) }
    }
}

struct AllowAll;

impl PermissionPolicy for AllowAll {
    fn check(&self, _tool_name: &str, _input: &serde_json::Value) -> PermissionDecision {
        PermissionDecision::Allow
    }
}

#[tokio::test]
async fn noop_hook_continues() {
    let hook = NoopHook;
    let event = HookEvent::LoopIteration { turn: 1 };
    let action = hook.on_event(event).await.unwrap();
    assert!(matches!(action, HookAction::Continue));
}

#[test]
fn allow_all_policy() {
    let policy = AllowAll;
    let decision = policy.check("bash", &serde_json::json!({"cmd": "ls"}));
    assert!(matches!(decision, PermissionDecision::Allow));
}
```

**Step 2: Run test to verify it fails**

Run: `cd agent-types && cargo test --test traits`
Expected: FAIL

**Step 3: Implement remaining traits**

In `src/traits.rs`, add: `ContextStrategy`, `ObservabilityHook` (with `HookEvent`, `HookAction`), `DurableContext` (with `ActivityOptions`, `RetryPolicy`), `PermissionPolicy` (with `PermissionDecision`) — all from design doc section 4.1.

Note: `HookError` needs to be added to error.rs:
```rust
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    #[error("hook failed: {0}")]
    Failed(String),
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}
```

**Step 4: Run tests**

Run: `cd agent-types && cargo test --test traits`
Expected: PASS

**Step 5: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): add ContextStrategy, hooks, DurableContext, PermissionPolicy traits"
```

---

### Task 1.11: Re-exports and final validation

**Files:**
- Modify: `agent-types/src/lib.rs`

**Step 1: Set up complete re-exports in lib.rs**

All public types and traits should be re-exported from `lib.rs` for ergonomic imports:

```rust
pub use types::*;
pub use traits::*;
pub use error::*;
pub use wasm::*;
pub use stream::*;
```

**Step 2: Run all tests**

Run: `cd agent-types && cargo test`
Expected: ALL PASS

**Step 3: Run clippy**

Run: `cd agent-types && cargo clippy -- -D warnings`
Expected: PASS (no warnings)

**Step 4: Write CLAUDE.md for the crate**

Create `agent-types/CLAUDE.md` with crate-specific guidance.

**Step 5: Commit**

```bash
git add agent-types/
git commit -m "feat(agent-types): finalize re-exports and crate docs"
```

---

## Block 2: `agent-tool` (Tool Registry + Middleware)

**Depends on:** `agent-types`

### Task 2.1: Initialize the Cargo project

**Files:**
- Create: `agent-tool/Cargo.toml`
- Create: `agent-tool/src/lib.rs`

**Step 1: Create project**

```toml
[package]
name = "agent-tool"
version = "0.1.0"
edition = "2024"
rust-version = "1.90"
description = "Tool registry and middleware pipeline for rust-agent-blocks"

[dependencies]
agent-types = { path = "../agent-types" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["sync"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
schemars = "0.8"
thiserror = "2"
tokio-util = "0.7"
```

**Step 2: Create module structure**

`src/lib.rs`, `src/registry.rs`, `src/middleware.rs`, `src/builtin.rs`

**Step 3: Verify compilation**

Run: `cd agent-tool && cargo check`

**Step 4: Commit**

```bash
git add agent-tool/
git commit -m "feat(agent-tool): initialize crate"
```

---

### Task 2.2: ToolCall type and Next struct

**Files:**
- Modify: `agent-tool/src/middleware.rs`
- Test: `agent-tool/tests/middleware.rs`

**Step 1: Write failing test**

```rust
use agent_tool::*;
use agent_types::*;

#[tokio::test]
async fn next_run_calls_tool() {
    // Test that Next::run correctly invokes the underlying tool
    // Will be tested through ToolRegistry.execute in Task 2.4
}
```

**Step 2: Implement ToolCall, ToolMiddleware trait, Next struct**

Implement exactly as in design doc section 4.3. `Next` holds a reference to the remaining middleware chain and the tool. `Next::run(self, ...)` consumes self to prevent double-invoke.

**Step 3: Implement `tool_middleware_fn` convenience**

```rust
pub fn tool_middleware_fn<F, Fut>(f: F) -> impl ToolMiddleware
where
    F: Fn(&ToolCall, &ToolContext, Next<'_>) -> Fut + Send + Sync,
    Fut: Future<Output = Result<ToolOutput, ToolError>> + Send;
```

**Step 4: Commit**

```bash
git add agent-tool/
git commit -m "feat(agent-tool): add ToolMiddleware trait with Next and tool_middleware_fn"
```

---

### Task 2.3: ToolRegistry core

**Files:**
- Modify: `agent-tool/src/registry.rs`
- Test: `agent-tool/tests/registry.rs`

**Step 1: Write failing test**

```rust
use agent_tool::*;
use agent_types::*;
use std::collections::HashMap;
use std::path::PathBuf;

// Reuse the ReadFileTool from agent-types tests
// ... (define ReadFileTool here)

#[tokio::test]
async fn register_and_execute_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);

    let ctx = ToolContext { /* ... */ };
    let result = registry.execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx).await.unwrap();
    assert!(!result.is_error);
}

#[test]
fn definitions_lists_all_tools() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    let defs = registry.definitions();
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "read_file");
}

#[tokio::test]
async fn execute_unknown_tool_returns_not_found() {
    let registry = ToolRegistry::new();
    let ctx = ToolContext { /* ... */ };
    let err = registry.execute("nonexistent", serde_json::json!({}), &ctx).await.unwrap_err();
    assert!(matches!(err, ToolError::NotFound(_)));
}
```

**Step 2: Implement ToolRegistry**

Implement `new()`, `register()`, `register_dyn()`, `get()`, `definitions()`, `execute()` as in design doc. `execute()` builds the middleware chain (global + per-tool) and calls through it.

**Step 3: Run tests**

Run: `cd agent-tool && cargo test`
Expected: PASS

**Step 4: Commit**

```bash
git add agent-tool/
git commit -m "feat(agent-tool): add ToolRegistry with register/execute"
```

---

### Task 2.4: Middleware chain execution

**Files:**
- Test: `agent-tool/tests/middleware_chain.rs`

**Step 1: Write failing test**

```rust
use agent_tool::*;
use agent_types::*;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

#[tokio::test]
async fn global_middleware_wraps_all_tools() {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    registry.add_middleware(tool_middleware_fn(move |call, ctx, next| {
        let c = counter_clone.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            next.run(call, ctx).await
        }
    }));

    let ctx = ToolContext { /* ... */ };
    registry.execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx).await.unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn per_tool_middleware_only_applies_to_named_tool() {
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    // Register another tool...
    registry.add_tool_middleware("read_file", tool_middleware_fn(move |call, ctx, next| {
        let c = counter_clone.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            next.run(call, ctx).await
        }
    }));

    let ctx = ToolContext { /* ... */ };
    registry.execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx).await.unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn middleware_can_short_circuit() {
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    registry.add_middleware(tool_middleware_fn(|_call, _ctx, _next| {
        async {
            // Don't call next — short-circuit
            Ok(ToolOutput {
                content: vec![ContentItem::Text("blocked".into())],
                structured_content: None,
                is_error: true,
            })
        }
    }));

    let ctx = ToolContext { /* ... */ };
    let result = registry.execute("read_file", serde_json::json!({"path": "/tmp/f"}), &ctx).await.unwrap();
    assert!(result.is_error);
}
```

**Step 2: Run, verify fails, implement chain logic, verify passes**

**Step 3: Commit**

```bash
git add agent-tool/
git commit -m "feat(agent-tool): implement middleware chain execution"
```

---

### Task 2.5: Built-in middleware (SchemaValidator, PermissionChecker, OutputFormatter)

**Files:**
- Modify: `agent-tool/src/builtin.rs`
- Test: `agent-tool/tests/builtin.rs`

**Step 1: Write tests for each**

Test `SchemaValidator` rejects invalid input. Test `PermissionChecker` delegates to a policy and blocks on Deny. Test `OutputFormatter` truncates long output.

**Step 2: Implement**

Three structs, each implementing `ToolMiddleware`.

**Step 3: Run tests, commit**

```bash
git add agent-tool/
git commit -m "feat(agent-tool): add built-in middleware (schema, permissions, output)"
```

---

### Task 2.6: Re-exports and crate docs

**Step 1: Finalize lib.rs re-exports**

**Step 2: Run full test suite + clippy**

Run: `cd agent-tool && cargo test && cargo clippy -- -D warnings`

**Step 3: Commit**

```bash
git add agent-tool/
git commit -m "feat(agent-tool): finalize re-exports and docs"
```

---

## Block 3: `agent-context` (Context Engine)

**Depends on:** `agent-types`

### Task 3.1: Initialize the Cargo project

Create `agent-context/` with deps on `agent-types`. Modules: `lib.rs`, `counter.rs`, `strategies.rs`, `persistent.rs`, `injector.rs`, `error.rs`.

Commit: `feat(agent-context): initialize crate`

---

### Task 3.2: TokenCounter

**Test:** Estimate tokens for text and messages. Verify `chars_per_token` ratio works.

**Implement:** `TokenCounter` with `estimate_messages()`, `estimate_text()`, `estimate_tools()` as in design doc section 4.5.

Commit: `feat(agent-context): add TokenCounter`

---

### Task 3.3: SlidingWindowStrategy

**Test:** Given 10 messages, compact to keep only the last 5. Verify system message is preserved.

**Implement:** `SlidingWindowStrategy` implementing `ContextStrategy`.

Commit: `feat(agent-context): add SlidingWindowStrategy`

---

### Task 3.4: ToolResultClearingStrategy

**Test:** Given messages with tool results, clear old ones but keep recent N.

**Implement:** `ToolResultClearingStrategy` implementing `ContextStrategy`. Replace old `ToolResult` content blocks with a short "[tool result cleared]" text.

Commit: `feat(agent-context): add ToolResultClearingStrategy`

---

### Task 3.5: SummarizationStrategy

**Test:** Use MockProvider to verify summarization flow. The strategy should call the provider to summarize old messages, then return the summary + recent messages.

**Implement:** `SummarizationStrategy<P: Provider>` implementing `ContextStrategy`.

Commit: `feat(agent-context): add SummarizationStrategy`

---

### Task 3.6: CompositeStrategy

**Test:** Chain SlidingWindow and ToolResultClearing. Verify both apply in order.

**Implement:** `CompositeStrategy` that tries each strategy until under threshold.

Commit: `feat(agent-context): add CompositeStrategy`

---

### Task 3.7: PersistentContext and SystemInjector

**Test:** Persistent sections survive compaction. Injector fires on trigger conditions.

**Implement:** `PersistentContext`, `ContextSection`, `SystemInjector`, `InjectionTrigger` from design doc.

Commit: `feat(agent-context): add PersistentContext and SystemInjector`

---

### Task 3.8: Re-exports and validation

Run: `cd agent-context && cargo test && cargo clippy -- -D warnings`

Commit: `feat(agent-context): finalize crate`

---

## Block 4: `agent-provider-anthropic` (First Real Provider)

**Depends on:** `agent-types`, `reqwest`, `serde`, `futures`

This validates the Provider trait against a real API.

### Task 4.1: Initialize the Cargo project

```toml
[dependencies]
agent-types = { path = "../agent-types" }
reqwest = { version = "0.12", features = ["json", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
futures = "0.3"
tracing = "0.1"
tokio = { version = "1", features = ["sync"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
wiremock = "0.6"
```

Modules: `lib.rs`, `client.rs`, `mapping.rs`, `streaming.rs`, `error.rs`

Commit: `feat(agent-provider-anthropic): initialize crate`

---

### Task 4.2: Anthropic client struct and builder

**Test:** Build client with API key and model. Verify defaults.

**Implement:**
```rust
pub struct Anthropic {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl Anthropic {
    pub fn new(api_key: impl Into<String>) -> Self;
    pub fn model(mut self, model: impl Into<String>) -> Self;
    pub fn base_url(mut self, url: impl Into<String>) -> Self;
}
```

Commit: `feat(agent-provider-anthropic): add Anthropic client builder`

---

### Task 4.3: Request/response mapping

**Test:** Map `CompletionRequest` to Anthropic API JSON format. Map Anthropic JSON response to `CompletionResponse`. Test system prompt mapping (Text → string, Blocks → array). Test thinking content blocks. Test cache_control on system blocks and tools.

**Implement:** `mapping.rs` with `to_api_request()` and `from_api_response()` functions. These are the core of the provider — translating our types to/from the Anthropic wire format.

Commit: `feat(agent-provider-anthropic): add request/response mapping`

---

### Task 4.4: Non-streaming completion (Provider::complete)

**Test:** Use `wiremock` to mock the Anthropic `/v1/messages` endpoint. Verify correct headers (`x-api-key`, `anthropic-version`, `content-type`). Verify request body. Verify response parsing.

**Implement:** `impl Provider for Anthropic` — the `complete` method. Makes a POST to `/v1/messages` with `stream: false`.

Commit: `feat(agent-provider-anthropic): implement Provider::complete`

---

### Task 4.5: Streaming completion (Provider::complete_stream)

**Test:** Use `wiremock` to serve SSE events. Verify TextDelta, ThinkingDelta, ToolUseStart/InputDelta/End, MessageComplete events are parsed correctly.

**Implement:** `streaming.rs` — parse SSE from Anthropic's streaming format. Each SSE `data:` line maps to a `StreamEvent`. Return a `StreamHandle` with a receiver.

Commit: `feat(agent-provider-anthropic): implement Provider::complete_stream`

---

### Task 4.6: Error mapping

**Test:** Map HTTP 429 to `ProviderError::RateLimit`. Map 401 to `ProviderError::Authentication`. Map 400 to `ProviderError::InvalidRequest`. Map network errors.

**Implement:** Error mapping from reqwest/HTTP errors to `ProviderError`.

Commit: `feat(agent-provider-anthropic): add error mapping`

---

### Task 4.7: Re-exports and validation

Run full suite + clippy. Write CLAUDE.md.

Commit: `feat(agent-provider-anthropic): finalize crate`

---

## Block 5: `agent-loop` (The Agentic While Loop)

**Depends on:** `agent-types`, `agent-tool`, `agent-context`

### Task 5.1: Initialize the Cargo project

```toml
[dependencies]
agent-types = { path = "../agent-types" }
agent-tool = { path = "../agent-tool" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
futures = "0.3"
tracing = "0.1"
tokio = { version = "1", features = ["sync"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
agent-context = { path = "../agent-context" }
```

Modules: `lib.rs`, `config.rs`, `loop_impl.rs`, `step.rs`

Commit: `feat(agent-loop): initialize crate`

---

### Task 5.2: LoopConfig and AgentLoop struct

**Test:** Build an AgentLoop with a MockProvider, empty ToolRegistry, and SlidingWindowStrategy. Verify config defaults.

**Implement:** `LoopConfig`, `AgentLoop<P, C>`, `AgentResult` as in design doc section 4.6.

Commit: `feat(agent-loop): add AgentLoop struct and config`

---

### Task 5.3: run() — basic loop to completion

**Test:** MockProvider returns text (no tool calls) → loop terminates after 1 turn. MockProvider returns tool_use → loop executes tool, sends result, MockProvider returns text → terminates after 2 turns. Test max_turns limit.

**Implement:** The core while loop as described in the design doc pseudocode. Without durability first — direct provider/tool calls.

Commit: `feat(agent-loop): implement run() with tool execution loop`

---

### Task 5.4: Hook integration

**Test:** ObservabilityHook receives PreLlmCall and PostLlmCall events. Hook returning `Terminate` stops the loop with `LoopError::HookTerminated`. Hook returning `Skip` on PreToolExecution skips the tool and returns a rejection message.

**Implement:** Fire hooks at each point in the loop. Respect `HookAction` return values.

Commit: `feat(agent-loop): integrate ObservabilityHook events`

---

### Task 5.5: Context compaction integration

**Test:** After N turns that exceed token threshold, compaction is triggered. Verify `ContextCompaction` hook event fires.

**Implement:** Check `context.should_compact()` at the top of each loop iteration. Call `context.compact()` when needed.

Commit: `feat(agent-loop): integrate context compaction`

---

### Task 5.6: DurableContext integration

**Test:** When durability is `Some`, calls go through `DurableContext::execute_llm_call` and `DurableContext::execute_tool` instead of direct calls. When `None`, calls go direct.

**Implement:** Add the `if durability` branches from the pseudocode.

Commit: `feat(agent-loop): integrate DurableContext`

---

### Task 5.7: run_stream() and run_step()

**Test:** `run_stream()` yields StreamEvents. `run_step()` yields `TurnResult` per turn, supports `inject_message()` and `tools_mut()`.

**Implement:** `run_stream()` wraps `complete_stream` and yields events. `StepIterator` yields after each turn.

Commit: `feat(agent-loop): implement run_stream() and run_step()`

---

### Task 5.8: Re-exports and validation

Run: `cd agent-loop && cargo test && cargo clippy -- -D warnings`

Commit: `feat(agent-loop): finalize crate`

---

## Block 6: `agent-mcp` (MCP Integration)

**Depends on:** `agent-types`, `rmcp`

### Task 6.1: Initialize the Cargo project

```toml
[dependencies]
agent-types = { path = "../agent-types" }
rmcp = { version = "0.1", features = ["client", "transport-io", "transport-child-process", "transport-streamable-http-client"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["process", "sync"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Modules: `lib.rs`, `client.rs`, `bridge.rs`, `server.rs`, `types.rs`, `error.rs`

Commit: `feat(agent-mcp): initialize crate`

---

### Task 6.2: McpClient with connect_stdio

**Test:** Integration test that connects to a simple MCP server process (if available) or a mock.

**Implement:** `McpClient` wrapping `rmcp::Client`. `connect_stdio()` spawns a child process and handles MCP initialization handshake.

Commit: `feat(agent-mcp): add McpClient with stdio transport`

---

### Task 6.3: McpClient with connect_http

**Implement:** `connect_http()` uses rmcp's Streamable HTTP transport.

Commit: `feat(agent-mcp): add HTTP transport`

---

### Task 6.4: Tool operations (list_tools, call_tool)

**Test:** Verify `list_tools` returns paginated results. Verify `call_tool` sends correct request.

**Implement:** Map between rmcp types and our types.

Commit: `feat(agent-mcp): add tool operations`

---

### Task 6.5: McpToolBridge (ToolDyn implementation)

**Test:** Bridge wraps an MCP tool as ToolDyn. Calling `call_dyn` forwards to `client.call_tool()`. Verify definition propagation.

**Implement:** `McpToolBridge` struct implementing `ToolDyn`. `discover_tools()` on `McpClient` returns `Vec<Arc<dyn ToolDyn>>`.

Commit: `feat(agent-mcp): add McpToolBridge for ToolDyn integration`

---

### Task 6.6: Resource and prompt operations

**Implement:** `list_resources`, `read_resource`, `list_prompts`, `get_prompt` on `McpClient`.

Commit: `feat(agent-mcp): add resource and prompt operations`

---

### Task 6.7: McpServer

**Implement:** `McpServer` wrapping a `ToolRegistry` and exposing tools via MCP protocol.

Commit: `feat(agent-mcp): add McpServer`

---

### Task 6.8: Re-exports and validation

Commit: `feat(agent-mcp): finalize crate`

---

## Block 7: `agent-provider-openai`

**Depends on:** `agent-types`, `reqwest`, `serde`, `futures`

Same pattern as agent-provider-anthropic.

### Task 7.1: Initialize crate

Commit: `feat(agent-provider-openai): initialize crate`

### Task 7.2: OpenAI client builder

Commit: `feat(agent-provider-openai): add client builder`

### Task 7.3: Request/response mapping

Map to OpenAI Chat Completions format. Key differences from Anthropic:
- `SystemPrompt` → `role: "developer"` message
- `ToolUse` → `tool_calls` array with `function` objects
- `response_format` → `type: "json_schema"` with schema object
- `reasoning_effort` → direct API parameter
- Multiple tool calls in one response (parallel tool calls)
- `extra` forwarded as additional body fields

Commit: `feat(agent-provider-openai): add request/response mapping`

### Task 7.4: Streaming

OpenAI SSE format: `choices[0].delta` with `content`, `tool_calls` fields.

Commit: `feat(agent-provider-openai): implement streaming`

### Task 7.5: Provider implementation + error mapping

Commit: `feat(agent-provider-openai): implement Provider trait`

### Task 7.6: Finalize

Commit: `feat(agent-provider-openai): finalize crate`

---

## Block 8: `agent-provider-ollama`

**Depends on:** `agent-types`, `reqwest`, `serde`, `futures`

### Task 8.1: Initialize crate

Commit: `feat(agent-provider-ollama): initialize crate`

### Task 8.2: Ollama client builder

Key config: `base_url` (default `http://localhost:11434`), `keep_alive`.

Commit: `feat(agent-provider-ollama): add client builder`

### Task 8.3: Request/response mapping

Key differences:
- `/api/chat` endpoint (not `/v1/...`)
- `max_tokens` → `options.num_predict`
- `temperature` → `options.temperature`
- Synthesize tool use IDs (Ollama doesn't provide them)
- `extra` forwarded into `options` object

Commit: `feat(agent-provider-ollama): add request/response mapping`

### Task 8.4: NDJSON streaming

Ollama uses newline-delimited JSON, not SSE. Parse each line as a JSON object.

Commit: `feat(agent-provider-ollama): implement NDJSON streaming`

### Task 8.5: Provider implementation + finalize

Commit: `feat(agent-provider-ollama): implement Provider and finalize`

---

## Block 9: `agent-runtime` (Production Layer)

**Depends on:** `agent-types`, `agent-tool`, `agent-loop`, `chrono`, `uuid`

Largest block (~2500 lines). Contains sub-agents, sessions, guardrails, DurableContext implementations, sandboxing.

### Task 9.1: Initialize crate

```toml
[dependencies]
agent-types = { path = "../agent-types" }
agent-tool = { path = "../agent-tool" }
agent-loop = { path = "../agent-loop" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
tokio = { version = "1", features = ["sync", "fs"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
agent-context = { path = "../agent-context" }
```

Modules: `lib.rs`, `session.rs`, `sub_agent.rs`, `guardrail.rs`, `durable.rs`, `sandbox.rs`, `error.rs`

Commit: `feat(agent-runtime): initialize crate`

### Task 9.2: Session and SessionState types

**Test:** Create, serialize, deserialize sessions. Verify timestamps.

**Implement:** `Session`, `SessionState`, `SessionSummary` from design doc.

Commit: `feat(agent-runtime): add Session types`

### Task 9.3: SessionStorage trait + in-memory impl

**Test:** Save and load sessions from in-memory storage. List sessions. Delete.

**Implement:** `SessionStorage` trait + `InMemorySessionStorage` implementation.

Commit: `feat(agent-runtime): add SessionStorage with in-memory impl`

### Task 9.4: File-based SessionStorage

**Test:** Save to disk, load back. Verify JSON files are created.

**Implement:** `FileSessionStorage` storing one JSON file per session.

Commit: `feat(agent-runtime): add file-based SessionStorage`

### Task 9.5: SubAgentConfig and SubAgentManager

**Test:** Spawn a sub-agent with MockProvider. Verify it gets filtered tools and its own system prompt. Verify max_depth prevents infinite nesting.

**Implement:** `SubAgentConfig`, `SubAgentManager` with `spawn()` from design doc.

Commit: `feat(agent-runtime): add SubAgentManager`

### Task 9.6: spawn_parallel

**Test:** Spawn 3 sub-agents in parallel. Verify all results collected.

**Implement:** `spawn_parallel()` using `tokio::join!` or `futures::join_all`.

Commit: `feat(agent-runtime): add parallel sub-agent spawning`

### Task 9.7: Guardrails

**Test:** InputGuardrail that rejects certain patterns. OutputGuardrail that flags secrets. Verify `Tripwire` stops execution.

**Implement:** `InputGuardrail`, `OutputGuardrail`, `GuardrailResult` traits from design doc.

Commit: `feat(agent-runtime): add guardrail traits`

### Task 9.8: LocalDurableContext

**Test:** LocalDurableContext passes through to provider and tools directly (no journaling). Verify `should_continue_as_new()` returns false. Verify `sleep()` works.

**Implement:** `LocalDurableContext` implementing `DurableContext` — direct passthrough.

Commit: `feat(agent-runtime): add LocalDurableContext`

### Task 9.9: Sandbox trait

**Test:** A mock sandbox wraps tool execution.

**Implement:** `Sandbox` trait from design doc.

Commit: `feat(agent-runtime): add Sandbox trait`

### Task 9.10: TemporalDurableContext and RestateDurableContext (stubs)

These require the actual SDKs (`temporal-sdk`, `restate-sdk`) and should be feature-gated. For now, implement as documented trait impls with the correct signatures and TODO bodies.

**Implement:** Feature-gated modules that compile but require actual SDK integration to use.

Commit: `feat(agent-runtime): add Temporal/Restate DurableContext stubs`

### Task 9.11: Finalize

Run: `cd agent-runtime && cargo test && cargo clippy -- -D warnings`

Commit: `feat(agent-runtime): finalize crate`

---

## Block 10: `agent-blocks` (Umbrella Crate)

**Depends on:** everything

### Task 10.1: Initialize and configure feature flags

```toml
[package]
name = "agent-blocks"
version = "0.1.0"
edition = "2024"

[features]
default = ["anthropic"]
anthropic = ["dep:agent-provider-anthropic"]
openai = ["dep:agent-provider-openai"]
ollama = ["dep:agent-provider-ollama"]
mcp = ["dep:agent-mcp"]
runtime = ["dep:agent-runtime"]
full = ["anthropic", "openai", "ollama", "mcp", "runtime"]

[dependencies]
agent-types = { path = "../agent-types" }
agent-tool = { path = "../agent-tool" }
agent-context = { path = "../agent-context" }
agent-loop = { path = "../agent-loop" }
agent-provider-anthropic = { path = "../agent-provider-anthropic", optional = true }
agent-provider-openai = { path = "../agent-provider-openai", optional = true }
agent-provider-ollama = { path = "../agent-provider-ollama", optional = true }
agent-mcp = { path = "../agent-mcp", optional = true }
agent-runtime = { path = "../agent-runtime", optional = true }
```

**Implement:** `lib.rs` with feature-gated re-exports.

Commit: `feat(agent-blocks): add umbrella crate with feature flags`

### Task 10.2: Integration examples

Write the three composition examples from design doc section 5 as integration tests:
- Minimal agent (3 blocks)
- Coding agent with MCP (6 blocks)
- Durable production agent (all blocks)

Commit: `feat(agent-blocks): add composition integration tests`

### Task 10.3: Final validation

Run all tests across all crates:

```bash
for crate in agent-types agent-tool agent-context agent-provider-anthropic agent-loop agent-mcp agent-provider-openai agent-provider-ollama agent-runtime agent-blocks; do
    (cd $crate && cargo test && cargo clippy -- -D warnings)
done
```

Commit: `chore: final validation pass`

---

## Verification

After all blocks are complete:

1. **Unit tests:** Every crate passes `cargo test`
2. **Clippy:** Every crate passes `cargo clippy -- -D warnings`
3. **Integration:** The three composition examples from the design doc compile and pass
4. **Dependency graph:** Verify no circular deps — each crate only depends on crates below it in the build order
5. **Doc comments:** `cargo doc --no-deps` generates clean documentation for each crate
6. **Manual smoke test:** Build a minimal agent with `agent-provider-anthropic` and run a single completion against the real API (requires `ANTHROPIC_API_KEY`)
