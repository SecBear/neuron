# agent-types

Foundation crate for rust-agent-blocks. Zero logic — pure types, traits, serde.

## Key types
- `Message`, `Role`, `ContentBlock`, `ContentItem` — conversation primitives
- `CompletionRequest`, `CompletionResponse`, `TokenUsage` — LLM API types
- `ToolDefinition`, `ToolOutput`, `ToolContext` — tool system types
- `StreamEvent`, `StreamHandle` — streaming types

## Key traits
- `Provider` — LLM provider (RPITIT, not object-safe, use generics)
- `Tool` — strongly typed tool, `ToolDyn` — type-erased for registry
- `ContextStrategy` — compaction strategy
- `ObservabilityHook` — logging/metrics hooks with Continue/Skip/Terminate
- `DurableContext` — wraps side effects for Temporal/Restate
- `PermissionPolicy` — tool call permission checks

## Conventions
- All public types re-exported from `lib.rs`
- `WasmCompatSend`/`WasmCompatSync` for WASM compat on all trait bounds
- `thiserror` for all error enums
- `schemars` for JSON Schema on tool args
- No logic in this crate — only types and trait definitions
