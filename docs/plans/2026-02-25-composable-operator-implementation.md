# Composable Operator Architecture — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rename Turn→Operator in layer0, split neuron-turn into toolkit + neuron-op-react, build OpenAI/Ollama providers and MCP, then audit and validate with a proof-of-concept.

**Architecture:** layer0 defines `trait Operator` (renamed from Turn). neuron-turn becomes the shared toolkit (Provider, types, ContextStrategy). The ReAct loop moves to neuron-op-react. New provider crates capture provider-specific features. neuron-mcp provides MCP client+server.

**Tech Stack:** Rust, serde, async-trait, reqwest, rmcp, tokio

**Build environment:**
```bash
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
export LIBRARY_PATH="/nix/store/7h6icyvqv6lqd0bcx41c8h3615rjcqb2-libiconv-109.100.2/lib"
```

**Verification command (run after every task):**
```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

---

## Phase A: Rename Turn → Operator in layer0

The core protocol rename. This touches every crate in the workspace. Do it first while it's free.

### Task A1: Rename types in layer0/src/turn.rs

**Files:**
- Modify: `layer0/src/turn.rs`

**Step 1: Rename the file content**

Apply these renames throughout the entire file (use replace_all):

| Old | New |
|-----|-----|
| `TurnInput` | `OperatorInput` |
| `TurnOutput` | `OperatorOutput` |
| `TurnConfig` | `OperatorConfig` |
| `TurnMetadata` | `OperatorMetadata` |
| `trait Turn` | `trait Operator` |

Keep `TriggerType`, `ExitReason`, and `ToolCallRecord` unchanged (they're not "Turn"-prefixed).

Update doc comments that say "turn" to say "operator" where referring to the trait/types. Keep lowercase "turn" where it refers to a model call (e.g., `turns_used` stays — it counts model calls).

**Step 2: Rename the module file**

Rename `layer0/src/turn.rs` to `layer0/src/operator.rs`.

**Step 3: Verify the file compiles in isolation**

```bash
# Will fail due to other files — just check for syntax errors
cargo check -p layer0 2>&1 | head -30
```

### Task A2: Update layer0 error types

**Files:**
- Modify: `layer0/src/error.rs`

**Step 1: Rename TurnError → OperatorError**

Replace all occurrences of `TurnError` with `OperatorError` throughout the file.

Update the `EnvError::TurnError` variant to `EnvError::OperatorError` and `OrchError::TurnError` to `OrchError::OperatorError`.

### Task A3: Update layer0 module wiring

**Files:**
- Modify: `layer0/src/lib.rs`
- Modify: `layer0/src/effect.rs`
- Modify: `layer0/src/orchestrator.rs`
- Modify: `layer0/src/environment.rs`

**Step 1: Update lib.rs**

Change `mod turn;` to `mod operator;` and update all re-exports:
- `Turn` → `Operator`
- `TurnInput` → `OperatorInput`
- `TurnOutput` → `OperatorOutput`
- `TurnConfig` → `OperatorConfig`
- `TurnMetadata` → `OperatorMetadata`
- `turn::` → `operator::`

**Step 2: Update effect.rs**

Change `use crate::turn::TurnInput` to `use crate::operator::OperatorInput`. Update the `Delegate` effect's field type.

**Step 3: Update orchestrator.rs**

Change imports from `crate::turn::{Turn, TurnInput, TurnOutput}` to `crate::operator::{Operator, OperatorInput, OperatorOutput}`. Update all uses in trait signature and doc comments.

**Step 4: Update environment.rs**

Change imports from `crate::turn::{Turn, TurnInput, TurnOutput}` to `crate::operator::{OperatorInput, OperatorOutput}`. Remove `Turn` from import if no longer needed (Environment::run no longer takes Arc<dyn Turn> — check current signature). Update doc comments.

### Task A4: Update layer0 test utilities

**Files:**
- Modify: `layer0/src/test_utils/echo_turn.rs` → rename to `echo_operator.rs`
- Modify: `layer0/src/test_utils/local_orchestrator.rs`
- Modify: `layer0/src/test_utils/local_environment.rs`
- Modify: `layer0/src/test_utils/mod.rs`

**Step 1: Rename echo_turn.rs to echo_operator.rs**

Rename the file. Change `EchoTurn` to `EchoOperator`. Update `impl Turn for EchoTurn` to `impl Operator for EchoOperator`. Update all type references.

**Step 2: Update local_orchestrator.rs**

Change `Arc<dyn Turn>` to `Arc<dyn Operator>`. Update imports.

**Step 3: Update local_environment.rs**

Change `Arc<dyn Turn>` to `Arc<dyn Operator>`. Update imports.

**Step 4: Update mod.rs**

Change `mod echo_turn;` to `mod echo_operator;` and update re-exports.

### Task A5: Update layer0 tests

**Files:**
- Modify: `layer0/tests/phase1.rs`
- Modify: `layer0/tests/phase2.rs`

**Step 1: Update phase1.rs**

Replace all occurrences (use replace_all for each):
- `Turn` → `Operator` (when referring to the trait)
- `TurnInput` → `OperatorInput`
- `TurnOutput` → `OperatorOutput`
- `TurnConfig` → `OperatorConfig`
- `TurnMetadata` → `OperatorMetadata`
- `TurnError` → `OperatorError`
- `EchoTurn` → `EchoOperator`

Be careful with test function names — rename `turn_` prefixed test names to `operator_` prefixed.

**Step 2: Update phase2.rs**

Same renames as phase1.rs. Also update `LocalOrchestrator` and `LocalEnvironment` usages if they reference Turn types.

**Step 3: Verify layer0 builds and tests pass**

```bash
cargo build -p layer0 --features test-utils && cargo test -p layer0 --features test-utils && cargo clippy -p layer0 --features test-utils -- -D warnings
```

### Task A6: Update neuron crates for Operator rename

**Files:**
- Modify: `neuron-turn/src/turn.rs`
- Modify: `neuron-turn/src/lib.rs`
- Modify: `neuron-turn/src/config.rs`
- Modify: `neuron-turn/src/convert.rs`
- Modify: `neuron-orch-local/src/lib.rs`
- Modify: `neuron-orch-local/tests/orch.rs`
- Modify: `neuron-env-local/src/lib.rs`
- Modify: `neuron-env-local/tests/env.rs`
- Modify: `neuron-provider-anthropic/tests/integration.rs`

**Step 1: Update neuron-turn**

In `neuron-turn/src/turn.rs`:
- Change `use layer0::turn::{...}` to `use layer0::operator::{...}` (or just `use layer0::{Operator, OperatorInput, ...}`)
- Change `impl<P: Provider + 'static> Turn for NeuronTurn<P>` to `impl<P: Provider + 'static> Operator for NeuronTurn<P>`
- Change all `TurnInput` → `OperatorInput`, `TurnOutput` → `OperatorOutput`, `TurnError` → `OperatorError`, `TurnMetadata` → `OperatorMetadata`, `TurnConfig` → `OperatorConfig`
- Update test functions that reference these types
- Update `NeuronTurnConfig` — keep this name for now (it's the neuron-specific config, not the protocol config)

In `neuron-turn/src/config.rs`:
- Update any references to `TurnConfig` → `OperatorConfig`

In `neuron-turn/src/convert.rs`:
- Update any Turn type references

In `neuron-turn/src/lib.rs`:
- Update re-exports

**Step 2: Update neuron-orch-local**

In `neuron-orch-local/src/lib.rs`:
- Change `Arc<dyn Turn>` to `Arc<dyn Operator>`
- Update imports from layer0
- Update `TurnInput`/`TurnOutput`/`TurnError` references

In `neuron-orch-local/tests/orch.rs`:
- Same renames

**Step 3: Update neuron-env-local**

In `neuron-env-local/src/lib.rs`:
- Change `Arc<dyn Turn>` to `Arc<dyn Operator>`
- Update imports

In `neuron-env-local/tests/env.rs`:
- Same renames

**Step 4: Update neuron-provider-anthropic integration tests**

In `neuron-provider-anthropic/tests/integration.rs`:
- Change `Turn` → `Operator`, `TurnInput` → `OperatorInput`, etc.

**Step 5: Full workspace verification**

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

Expected: 218+ tests pass, zero warnings.

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: rename Turn → Operator across entire workspace

The protocol trait is now 'Operator' — an atomic unit of agentic work.
'Turn' now exclusively means one model call (the natural meaning).
Renames: Turn→Operator, TurnInput→OperatorInput, TurnOutput→OperatorOutput,
TurnConfig→OperatorConfig, TurnMetadata→OperatorMetadata, TurnError→OperatorError,
EchoTurn→EchoOperator. All 218+ tests pass."
```

---

## Phase B: Split neuron-turn → toolkit + neuron-op-react

Extract the ReAct loop from neuron-turn into neuron-op-react. neuron-turn becomes the shared toolkit.

### Task B1: Create neuron-op-react crate

**Files:**
- Create: `neuron-op-react/Cargo.toml`
- Create: `neuron-op-react/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "neuron-op-react"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "ReAct operator for neuron — model + tools in a loop"

[dependencies]
layer0 = { path = "../layer0" }
neuron-turn = { path = "../neuron-turn" }
neuron-tool = { path = "../neuron-tool" }
neuron-hooks = { path = "../neuron-hooks" }
async-trait = "0.1"
serde_json = "1"
rust_decimal = { version = "1", features = ["serde-str"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

**Step 2: Move the ReAct loop from neuron-turn/src/turn.rs to neuron-op-react/src/lib.rs**

Move these items from `neuron-turn/src/turn.rs`:
- `NeuronTurn<P>` struct → rename to `ReactOperator<P>`
- `NeuronTurnConfig` → rename to `ReactConfig`
- `impl<P: Provider + 'static> Operator for NeuronTurn<P>` → `impl<P: Provider + 'static> Operator for ReactOperator<P>`
- The `make_output` helper function
- All internal helper methods on `NeuronTurn`
- All `#[cfg(test)]` module contents (mock types, test functions)

The file should start with:
```rust
#![deny(missing_docs)]
//! ReAct operator — model + tools in a reasoning loop.
//!
//! Implements `layer0::Operator` by running the Reason-Act-Observe cycle:
//! assemble context → call model → execute tools → repeat until done.

use layer0::operator::{Operator, OperatorInput, OperatorOutput, OperatorConfig, OperatorMetadata, ExitReason, ToolCallRecord};
use layer0::error::OperatorError;
use layer0::effect::Effect;
use layer0::hook::{HookPoint, HookAction, HookContext};
use layer0::content::{Content, ContentBlock};
use layer0::id::Scope;
use layer0::StateReader;
use neuron_turn::provider::{Provider, ProviderError};
use neuron_turn::types::*;
use neuron_turn::context::ContextStrategy;
use neuron_turn::convert::*;
use neuron_tool::ToolRegistry;
use neuron_hooks::HookRegistry;
use rust_decimal::Decimal;
use std::sync::Arc;
```

**Step 3: Add to workspace**

Add `"neuron-op-react"` to workspace members in root `Cargo.toml`.

**Step 4: Verify it compiles**

```bash
cargo build -p neuron-op-react
```

### Task B2: Clean up neuron-turn as toolkit

**Files:**
- Modify: `neuron-turn/src/lib.rs`
- Modify: `neuron-turn/src/turn.rs` (delete or gut)
- Modify: `neuron-turn/Cargo.toml`

**Step 1: Remove NeuronTurn from neuron-turn**

After moving the ReAct loop to neuron-op-react, `neuron-turn/src/turn.rs` should be deleted or emptied. The file contained the NeuronTurn struct and the ReAct loop — all of which moved to neuron-op-react.

**Step 2: Update neuron-turn/src/lib.rs**

neuron-turn now exports ONLY the toolkit:
```rust
#![deny(missing_docs)]
//! Shared toolkit for building operators.
//!
//! Provides the [`Provider`] trait for making model calls,
//! [`ContextStrategy`] for managing context between calls,
//! and all the types needed by operator implementations.

pub mod provider;
pub mod types;
pub mod context;
pub mod convert;
```

No more `mod turn;` or `NeuronTurn` re-export.

**Step 3: Remove unnecessary dependencies from neuron-turn/Cargo.toml**

neuron-turn no longer needs neuron-tool, neuron-hooks, or layer0 as direct dependencies (only types). Check what's actually needed for Provider/types/context/convert and keep only those.

Actually — neuron-turn DOES need layer0 because convert.rs converts between layer0::Content and ContentPart. Keep layer0. Remove neuron-tool and neuron-hooks (those are only needed by the ReAct loop, now in neuron-op-react).

**Step 4: Update neuron-context dependency**

neuron-context depends on neuron-turn for ContextStrategy. Verify it still compiles:
```bash
cargo build -p neuron-context
```

**Step 5: Update neuron-provider-anthropic**

The integration test imports NeuronTurn. Update to import ReactOperator from neuron-op-react instead. Add neuron-op-react as a dev-dependency.

In `neuron-provider-anthropic/Cargo.toml` [dev-dependencies], add:
```toml
neuron-op-react = { path = "../neuron-op-react" }
```

In `neuron-provider-anthropic/tests/integration.rs`, change:
```rust
// Old:
use neuron_turn::NeuronTurn;
use neuron_turn::NeuronTurnConfig;
// New:
use neuron_op_react::{ReactOperator, ReactConfig};
```

**Step 6: Full workspace verification**

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

Expected: Same test count as before (tests moved from neuron-turn to neuron-op-react).

**Step 7: Commit**

```bash
git add -A
git commit -m "refactor: split neuron-turn into toolkit + neuron-op-react

neuron-turn is now the shared toolkit (Provider, types, ContextStrategy).
The ReAct loop moves to neuron-op-react as ReactOperator<P>.
NeuronTurn<P> → ReactOperator<P>, NeuronTurnConfig → ReactConfig."
```

---

## Phase C: New Provider Crates

### Task C1: Build neuron-provider-openai

**Files:**
- Create: `neuron-provider-openai/Cargo.toml`
- Create: `neuron-provider-openai/src/lib.rs`
- Create: `neuron-provider-openai/src/types.rs`
- Create: `neuron-provider-openai/tests/integration.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "neuron-provider-openai"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "OpenAI API provider for neuron-turn"

[dependencies]
neuron-turn = { path = "../neuron-turn" }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rust_decimal = { version = "1", features = ["serde-str"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
layer0 = { path = "../layer0" }
neuron-op-react = { path = "../neuron-op-react" }
neuron-tool = { path = "../neuron-tool" }
neuron-hooks = { path = "../neuron-hooks" }
neuron-context = { path = "../neuron-context" }
neuron-state-memory = { path = "../neuron-state-memory" }
```

**Step 2: Create types.rs**

OpenAI Chat Completions API types. Key differences from Anthropic:
- Messages use `role` field with "system", "user", "assistant", "developer", "tool" values
- Content can be string or array of content parts
- Tool calls are in a `tool_calls` array on assistant messages (not inline content blocks)
- Tool results use role="tool" with `tool_call_id` field
- Tool call arguments are JSON strings (must parse), not JSON objects
- `parallel_tool_calls` flag
- `service_tier` field
- `reasoning_effort` for o-series models
- `response_format` for structured outputs

```rust
//! OpenAI Chat Completions API request/response types.

use serde::{Deserialize, Serialize};

/// OpenAI API request body.
#[derive(Debug, Serialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<OpenAITool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
}

/// A message in OpenAI format.
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<OpenAIContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Content can be a string or array of parts.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAIContent {
    Text(String),
    Parts(Vec<OpenAIContentPart>),
}

/// A content part.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OpenAIContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: OpenAIImageUrl },
}

/// Image URL reference.
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A tool call from the assistant.
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: OpenAIFunctionCall,
}

/// Function call details.
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAIFunctionCall {
    pub name: String,
    /// JSON string — must be parsed by the caller.
    pub arguments: String,
}

/// Tool definition for OpenAI.
#[derive(Debug, Serialize)]
pub struct OpenAITool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAIFunction,
}

/// Function definition.
#[derive(Debug, Serialize)]
pub struct OpenAIFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// OpenAI API response.
#[derive(Debug, Deserialize)]
pub struct OpenAIResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<OpenAIChoice>,
    pub usage: OpenAIUsage,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub system_fingerprint: Option<String>,
}

/// A choice in the response.
#[derive(Debug, Deserialize)]
pub struct OpenAIChoice {
    pub index: u32,
    pub message: OpenAIMessage,
    pub finish_reason: String,
}

/// Token usage.
#[derive(Debug, Deserialize)]
pub struct OpenAIUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub prompt_tokens_details: Option<OpenAIPromptTokensDetails>,
    #[serde(default)]
    pub completion_tokens_details: Option<OpenAICompletionTokensDetails>,
}

/// Prompt token breakdown.
#[derive(Debug, Deserialize)]
pub struct OpenAIPromptTokensDetails {
    #[serde(default)]
    pub cached_tokens: Option<u64>,
}

/// Completion token breakdown.
#[derive(Debug, Deserialize)]
pub struct OpenAICompletionTokensDetails {
    #[serde(default)]
    pub reasoning_tokens: Option<u64>,
}
```

**Step 3: Create lib.rs**

Implement `OpenAIProvider` struct with `impl Provider`:
- `new(api_key)` constructor
- `with_url(url)` for testing/proxies
- `with_org(org_id)` for organization header
- `build_request` — map ProviderRequest → OpenAIRequest
  - System prompt becomes a system/developer message (first message)
  - Tool schemas wrap in `{type: "function", function: {...}}`
  - `extra` field can carry service_tier, reasoning_effort, parallel_tool_calls
- `parse_response` — map OpenAIResponse → ProviderResponse
  - Parse tool_calls from assistant message into ContentPart::ToolUse
  - Parse string arguments into serde_json::Value
  - Map finish_reason: "stop" → EndTurn, "tool_calls" → ToolUse, "length" → MaxTokens, "content_filter" → ContentFilter
  - Cost calculation (varies by model — use extra field or hardcoded table for common models)
  - Extract reasoning_tokens from completion_tokens_details

Follow the same structure as neuron-provider-anthropic/src/lib.rs (see design doc section 7 for OpenAI-specific features).

**Step 4: Write unit tests**

Same pattern as Anthropic: build_simple_request, parse_simple_response, parse_tool_use_response, tool_schema_serializes. Plus:
- `parse_string_tool_arguments` — verify JSON string args are parsed to Value
- `service_tier_passed_through` — verify extra field handling

**Step 5: Write integration test**

```rust
// tests/integration.rs
#[tokio::test]
#[ignore] // Requires OPENAI_API_KEY
async fn real_openai_simple_completion() {
    // Same structure as Anthropic integration test
    // Use gpt-4o-mini for cheapness
}
```

**Step 6: Add to workspace and verify**

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

**Step 7: Commit**

```bash
git add neuron-provider-openai/ Cargo.toml
git commit -m "feat: add neuron-provider-openai with OpenAI-specific features

Supports Chat Completions API, string tool arguments, service tiers,
reasoning_effort for o-series models, parallel_tool_calls, prompt caching
token details. 6+ unit tests."
```

### Task C2: Build neuron-provider-ollama

**Files:**
- Create: `neuron-provider-ollama/Cargo.toml`
- Create: `neuron-provider-ollama/src/lib.rs`
- Create: `neuron-provider-ollama/src/types.rs`
- Create: `neuron-provider-ollama/tests/integration.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create Cargo.toml**

Same structure as OpenAI but no auth needed. Default URL: `http://localhost:11434`.

```toml
[package]
name = "neuron-provider-ollama"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "Ollama local model provider for neuron-turn"

[dependencies]
neuron-turn = { path = "../neuron-turn" }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rust_decimal = { version = "1", features = ["serde-str"] }
uuid = { version = "1", features = ["v4"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
layer0 = { path = "../layer0" }
neuron-op-react = { path = "../neuron-op-react" }
neuron-tool = { path = "../neuron-tool" }
neuron-hooks = { path = "../neuron-hooks" }
neuron-context = { path = "../neuron-context" }
neuron-state-memory = { path = "../neuron-state-memory" }
```

**Step 2: Create types.rs**

Ollama /api/chat types. Key differences:
- Endpoint: POST /api/chat (not /v1/chat/completions)
- No auth headers
- `stream: false` for non-streaming
- Messages use `role` field: "system", "user", "assistant"
- Tool calls use `tool_calls` array with function object containing **object** arguments (not string!)
- Tool results use role="tool"
- Response has timing metadata: `total_duration`, `load_duration`, `prompt_eval_count`, `prompt_eval_duration`, `eval_count`, `eval_duration` (all in nanoseconds)
- `keep_alive` parameter controls model memory persistence
- `options` object for hardware tuning (num_gpu, num_thread, temperature, etc.)
- No tool_use IDs from Ollama — must synthesize UUIDs

```rust
//! Ollama /api/chat request/response types.
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct OllamaRequest {
    pub model: String,
    pub messages: Vec<OllamaMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<OllamaTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_alive: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaToolCall {
    pub function: OllamaFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OllamaFunctionCall {
    pub name: String,
    /// Object arguments — NOT a string like OpenAI.
    pub arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct OllamaTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OllamaFunction,
}

#[derive(Debug, Serialize)]
pub struct OllamaFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_gpu: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_thread: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct OllamaResponse {
    pub model: String,
    pub message: OllamaMessage,
    pub done: bool,
    #[serde(default)]
    pub done_reason: Option<String>,
    #[serde(default)]
    pub total_duration: Option<u64>,
    #[serde(default)]
    pub load_duration: Option<u64>,
    #[serde(default)]
    pub prompt_eval_count: Option<u64>,
    #[serde(default)]
    pub prompt_eval_duration: Option<u64>,
    #[serde(default)]
    pub eval_count: Option<u64>,
    #[serde(default)]
    pub eval_duration: Option<u64>,
}
```

**Step 3: Create lib.rs**

Implement `OllamaProvider`:
- `new()` — defaults to `http://localhost:11434`
- `with_url(url)` — custom endpoint
- `with_keep_alive(duration)` — model persistence
- `build_request` — map ProviderRequest → OllamaRequest
  - System prompt becomes a system message (first message)
  - Set `stream: false`
  - Hardware options from `extra` field
- `parse_response` — map OllamaResponse → ProviderResponse
  - Synthesize UUIDs for tool calls (Ollama doesn't provide IDs)
  - Map done_reason: "stop" → EndTurn, tool_calls present → ToolUse
  - Use eval_count as output_tokens, prompt_eval_count as input_tokens
  - Cost: always zero (local inference)
  - Store timing metadata in extra if useful

**Step 4: Write unit tests**

Same pattern. Plus: `synthesized_tool_ids_are_unique`, `object_arguments_preserved`, `timing_metadata_parsed`.

**Step 5: Write integration test (requires local Ollama)**

```rust
#[tokio::test]
#[ignore] // Requires local Ollama running
async fn real_ollama_simple_completion() {
    // Use a small model like "llama3.2:1b" or "phi3:mini"
}
```

**Step 6: Verify and commit**

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
git add neuron-provider-ollama/ Cargo.toml
git commit -m "feat: add neuron-provider-ollama with Ollama-specific features

Supports /api/chat endpoint, object tool arguments, synthesized tool IDs,
nanosecond timing metadata, keep_alive, hardware options. Zero cost. 6+ tests."
```

---

## Phase D: neuron-op-single-shot

### Task D1: Create neuron-op-single-shot

**Files:**
- Create: `neuron-op-single-shot/Cargo.toml`
- Create: `neuron-op-single-shot/src/lib.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "neuron-op-single-shot"
version = "0.1.0"
edition.workspace = true
license.workspace = true
description = "Single-shot operator — one model call, no tools, return immediately"

[dependencies]
layer0 = { path = "../layer0" }
neuron-turn = { path = "../neuron-turn" }
async-trait = "0.1"
rust_decimal = { version = "1", features = ["serde-str"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

**Step 2: Create lib.rs**

The simplest operator: one model call, no tools, return result.

```rust
#![deny(missing_docs)]
//! Single-shot operator — one model call, no tools, return immediately.
//!
//! Use for classification, summarization, or any task that needs
//! one inference call without tool use or looping.

use layer0::operator::{
    Operator, OperatorInput, OperatorOutput, OperatorMetadata, ExitReason,
};
use layer0::error::OperatorError;
use layer0::content::Content;
use neuron_turn::provider::{Provider, ProviderError};
use neuron_turn::types::*;
use neuron_turn::convert;
use rust_decimal::Decimal;
use std::sync::Arc;

/// Configuration for the single-shot operator.
pub struct SingleShotConfig {
    /// System prompt.
    pub system_prompt: String,
    /// Default model identifier.
    pub default_model: String,
    /// Default max tokens.
    pub default_max_tokens: u32,
}

/// Single-shot operator — one model call, return result.
pub struct SingleShotOperator<P: Provider> {
    provider: P,
    config: SingleShotConfig,
}

impl<P: Provider> SingleShotOperator<P> {
    /// Create a new single-shot operator.
    pub fn new(provider: P, config: SingleShotConfig) -> Self {
        Self { provider, config }
    }
}

#[async_trait::async_trait]
impl<P: Provider + 'static> Operator for SingleShotOperator<P> {
    async fn execute(&self, input: OperatorInput) -> Result<OperatorOutput, OperatorError> {
        let model = input.config.as_ref()
            .and_then(|c| c.model.clone())
            .unwrap_or_else(|| self.config.default_model.clone());
        let max_tokens = input.config.as_ref()
            .and_then(|c| c.max_turns.map(|_| self.config.default_max_tokens))
            .unwrap_or(self.config.default_max_tokens);

        let user_message = convert::content_to_user_message(&input.message);

        let request = ProviderRequest {
            model: Some(model),
            messages: vec![user_message],
            tools: vec![],
            max_tokens: Some(max_tokens),
            temperature: None,
            system: Some(self.config.system_prompt.clone()),
            extra: serde_json::Value::Null,
        };

        let response = self.provider.complete(request).await.map_err(|e| match e {
            ProviderError::RateLimited => OperatorError::Retryable("rate limited".into()),
            ProviderError::AuthFailed(msg) => OperatorError::NonRetryable(msg),
            other => OperatorError::ContextAssembly(other.to_string()),
        })?;

        let content = convert::parts_to_content(&response.content);

        Ok(OperatorOutput {
            message: content,
            exit_reason: ExitReason::Complete,
            metadata: OperatorMetadata {
                tokens_in: response.usage.input_tokens,
                tokens_out: response.usage.output_tokens,
                cost: response.cost.unwrap_or(Decimal::ZERO),
                turns_used: 1,
                tools_called: vec![],
                duration: layer0::DurationMs(0),
            },
            effects: vec![],
        })
    }
}
```

Note: The exact field names and constructor patterns depend on what layer0 types look like after the rename. Adjust as needed during implementation.

**Step 3: Write tests**

Test with a mock provider (same pattern as neuron-op-react tests):
- `single_shot_returns_completion`
- `single_shot_always_one_turn`
- `single_shot_no_tools_in_request`
- `single_shot_rate_limit_maps_to_retryable`

**Step 4: Verify and commit**

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
git add neuron-op-single-shot/ Cargo.toml
git commit -m "feat: add neuron-op-single-shot — simplest operator, one model call"
```

---

## Phase E: neuron-mcp

### Task E1: Research rmcp SDK and create neuron-mcp

**Files:**
- Create: `neuron-mcp/Cargo.toml`
- Create: `neuron-mcp/src/lib.rs`
- Create: `neuron-mcp/src/client.rs`
- Create: `neuron-mcp/src/server.rs`
- Modify: `Cargo.toml` (workspace)

**Step 1: Research rmcp**

Before writing code, read the rmcp documentation and examples:
- Crate: `rmcp` on crates.io (the official Rust MCP SDK from `modelcontextprotocol/rust-sdk`)
- Study `ClientHandler` and `ServerHandler` traits
- Study transport options (stdio, SSE, streamable HTTP)
- Study the `#[tool]` macro for defining tools

**Step 2: Design neuron-mcp**

Two independent components:

**MCP Client** — discovers and uses remote tools:
```rust
pub struct McpClient {
    // Wraps rmcp Peer<ClientRole>
    // Discovers tools from MCP server
    // Registers them into ToolRegistry
}

impl McpClient {
    pub async fn connect_stdio(command: &str, args: &[&str]) -> Result<Self, McpError>;
    pub async fn connect_sse(url: &str) -> Result<Self, McpError>;
    pub fn tools(&self) -> Vec<McpTool>;  // returns discovered tools
}
```

**MCP Server** — exposes neuron capabilities:
```rust
pub struct McpServer {
    // Wraps rmcp ServerHandler
    // Exposes ToolRegistry as MCP tools
}

impl McpServer {
    pub fn new(tools: ToolRegistry) -> Self;
    pub async fn serve_stdio(self) -> Result<(), McpError>;
}
```

**Step 3: Implement McpClient**

The client connects to an MCP server, discovers tools, and wraps each discovered tool as a `ToolDyn` that can be registered in `ToolRegistry`.

**Step 4: Implement McpServer**

The server wraps a `ToolRegistry` and serves its tools via MCP protocol.

**Step 5: Write tests**

- Unit test: McpTool wraps correctly as ToolDyn
- Integration test (ignored): connect client to a test server via stdio

**Step 6: Verify and commit**

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
git add neuron-mcp/ Cargo.toml
git commit -m "feat: add neuron-mcp — MCP client and server

Client discovers and registers remote MCP tools into ToolRegistry.
Server exposes ToolRegistry as MCP tools. Both independently composable."
```

---

## Phase F: Comprehensive Audit and Test Coverage

### Task F1: Audit layer0 test coverage

**Files:**
- Modify: `layer0/tests/phase1.rs`
- Modify: `layer0/tests/phase2.rs`

**Step 1: Add missing Arc object safety tests**

Test `Arc<dyn T>` is Send + Sync for all traits: Operator, Orchestrator, StateStore, StateReader, Environment, Hook.

**Step 2: Add error Display tests for all variants**

Every error variant (OperatorError, OrchError, StateError, EnvError, HookError) should have a Display test.

**Step 3: Add serde roundtrip tests for all remaining types**

HookAction variants (Halt, SkipTool, ModifyToolInput), BudgetEvent variants, BudgetDecision variants, ExitReason all variants.

**Step 4: Add Orchestrator signal/query tests**

Test LocalOrchestrator::signal and ::query.

**Step 5: Verify**

```bash
cargo test -p layer0 --features test-utils
```

### Task F2: Audit neuron-turn toolkit tests

**Files:**
- Modify: neuron-turn test files

Test Provider trait, ProviderRequest/Response serialization, ContentPart conversions, ContextStrategy interface.

### Task F3: Audit neuron-op-react tests

Ensure all 23 existing tests migrated correctly. Add any missing edge cases.

### Task F4: Audit provider crate tests

Ensure each provider has: build_request, parse_response, parse_tool_use, error handling tests.

### Task F5: Commit audit results

```bash
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
git add -A
git commit -m "test: comprehensive coverage audit across all crates"
```

---

## Phase G: Real API Integration Tests

### Task G1: Anthropic integration test

Run the existing ignored test with a real API key:
```bash
ANTHROPIC_API_KEY=<key> cargo test -p neuron-provider-anthropic --ignored
```

### Task G2: OpenAI integration test

```bash
OPENAI_API_KEY=<key> cargo test -p neuron-provider-openai --ignored
```

### Task G3: Ollama integration test

```bash
# Requires local Ollama running with a model pulled
cargo test -p neuron-provider-ollama --ignored
```

### Task G4: Cross-provider integration

Write a test that runs the same prompt through all three providers and verifies consistent OperatorOutput structure.

---

## Phase H: Proof of Concept

### Task H1: Build proof-of-concept

**Files:**
- Create: `examples/poc-research-reviewer/` or `tests/poc.rs`

Build a small agentic system demonstrating:
1. **Provider swap**: same workflow, swap Anthropic ↔ OpenAI ↔ Ollama
2. **State swap**: same workflow, swap memory ↔ filesystem state
3. **Operator swap**: same workflow, swap react ↔ single-shot
4. **Multi-agent**: orchestrator dispatches to researcher + reviewer

### Task H2: Write findings

Document what worked, what was awkward, what needs improvement.

---

## Verification Checklist (Every Phase)

```bash
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
export LIBRARY_PATH="/nix/store/7h6icyvqv6lqd0bcx41c8h3615rjcqb2-libiconv-109.100.2/lib"
cargo build && cargo test && cargo clippy -- -D warnings && cargo doc --no-deps
```

All four must pass before any commit.
