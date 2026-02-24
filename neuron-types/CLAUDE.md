# neuron-types

Foundation crate for neuron. Zero logic — pure types, traits, serde.

## Key types
- `Message`, `Role`, `ContentBlock`, `ContentItem` — conversation primitives.
  `Message` has `user()`, `assistant()`, `system()` convenience constructors for single-text messages.
- `CompletionRequest`, `CompletionResponse`, `TokenUsage` — LLM API types
- `ContextManagement`, `ContextEdit` — server-side context management config
- `UsageIteration` — per-iteration token usage (during server-side compaction)
- `ToolDefinition`, `ToolOutput`, `ToolContext` — tool system types.
  `ToolContext` implements `Default` (cwd from env, empty session/environment, fresh cancellation token).
- `EmbeddingRequest`, `EmbeddingResponse`, `EmbeddingUsage` — embedding API types
- `StreamEvent`, `StreamHandle`, `StreamError` — streaming types
- `SystemPrompt`, `SystemBlock` — system prompt configuration
- `CacheControl`, `CacheTtl` — prompt caching configuration
- `ToolChoice` — tool selection mode (auto, any, specific, none)
- `ResponseFormat` — structured output format (json_schema)
- `ThinkingConfig`, `ReasoningEffort` — extended thinking configuration
- `ImageSource`, `DocumentSource` — multi-modal content sources
- `ProgressReporter` — tool execution progress callback
- `HookEvent`, `HookAction` — observability hook event and action types
- `ActivityOptions`, `RetryPolicy` — durable execution configuration
- `PermissionDecision` — permission check result (Allow, Deny, Ask)
- `UsageLimits` — token usage limits (request tokens, response tokens, total tokens) checked by the agentic loop

## Error types
- `ProviderError` — LLM provider errors with `is_retryable()` classification
- `ToolError` — tool execution errors (`NotFound`, `InvalidInput`, `ExecutionFailed`, `PermissionDenied`, `Cancelled`)
- `LoopError` — agentic loop errors (`Provider`, `Tool`, `Context`, `MaxTurns`, `HookTerminated`, `Cancelled`, `UsageLimitExceeded`)
- `ContextError` — context compaction errors
- `DurableError` — durable execution errors
- `HookError` — observability hook errors
- `McpError` — MCP protocol errors
- `StorageError` — session storage errors
- `EmbeddingError` — embedding provider errors
- `SandboxError` — sandbox execution errors

## Key traits
- `Provider` — LLM provider (RPITIT, not object-safe, use generics)
- `EmbeddingProvider` — embedding model provider (RPITIT, separate from Provider)
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
