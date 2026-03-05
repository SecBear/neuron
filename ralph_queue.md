# ralph_queue.md

This file is the single "what next" queue used by the Ralph loop (`PROMPT.md`).

Rules:

1. Keep it short.
2. Each item must link to the governing spec(s).
3. Each item must have a concrete "done when" and a verification command.

- ReleasePrep: Prepare merge + publish
  - Done when: RELEASE_NOTES.md and MIGRATION.md present; crate versions bumped coherently; publish.yml dry-run succeeds
  - Verify: GH workflow `publish (manual)` with `dry-run: true` passes; CI green on PR

## Queue


## Completed

- 2026-03-03: V32-23 — Spec refresh — operator/turn runtime (spec 04)
  - Adds:
    - `specs/04-operator-turn-runtime.md`: SafetyStop exit row; TieredStrategy zone table (Pinned/Active/Summary/Noise); AnnotatedMessage/CompactionPolicy context assembly section; BudgetEvent (8 variants) and CompactionEvent (8 variants) with firing conditions; pre-compaction flush L2↔D2C bridge; exit reason table with all 8 variants
  - Verify: Doc review only (no code changes)

- 2026-03-03: V32-24 — Spec refresh — state core (spec 07)
  - Adds:
    - `specs/07-state-core.md`: Full rewrite (270 lines). StoreOptions 5-field table with backend guidance; MemoryTier/Lifetime/ContentKind enums with semantic guidance; AnnotatedMessage section with cross-reference to spec 04; API surface (write_hinted/read_hinted/clear_transient)
  - Verify: Doc review only

- 2026-03-03: V32-25 — Spec refresh — hooks and lifecycle (spec 09)
  - Adds:
    - `specs/09-hooks-lifecycle-and-governance.md`: Hook points table with Key Context Fields column (steering_messages, skipped_tools, memory_key/value/options, model_output, tool_chunk); HookKind dispatch order (observers→transformers→guardrails) with phase semantics; BudgetEvent 8-variant table; CompactionEvent 8-variant table; implementation status corrected
  - Verify: Doc review only

- 2026-03-03: V32-26 — Spec refresh — effects (spec 03)
  - Adds:
    - `specs/03-effects-and-execution-semantics.md`: Effect Variants section with all 7 WriteMemory fields (scope, key, value, tier, lifetime, content_kind, salience, ttl) as struct literal + field table; all other Effect variants documented; Executor Guidance section (MUST use write_hinted() with StoreOptions, MAY ignore any field); stale 'required next step' note removed
  - Verify: Doc review only

- 2026-03-03: V32-27 — Constitution sync with golden framework v3.2
  - Adds:
    - `CONSTITUTION.md`: L2 (TieredStrategy + recursive degradation + AnnotatedMessage/CompactionPolicy); D2B (SaveSnapshot/LoadSnapshot); D2D (API-to-MCP antipattern); C4 (AG-UI cross-protocol); L5 (context introspection as unimplemented emerging requirement); Open Questions section (L7 eval candidate, C6, L6, hook cross-type composition)
    - `docs/architecture/agentic-decision-map-v3.md`: D2B serialized snapshots row; D2D antipattern paragraph; D3C safety stops language; D5 safety stop row; L2 tiered compaction row + recursive degradation + message-level metadata paragraphs; L5 context introspection row + paragraph
  - Verify: Doc review only

- 2026-03-03: V32-28 — ExitReason::SafetyStop variant
  - Specs: `specs/04-operator-turn-runtime.md`
  - Adds:
    - `layer0/src/operator.rs`: `SafetyStop { reason: String }` variant on `ExitReason` with doc comment (HTTP 200, distinct from Error/Complete, not retriable without context modification)
    - `op/neuron-op-react/src/lib.rs`: `StopReason::ContentFilter` → `ExitReason::SafetyStop { reason: "content_filter" }` (was Err(OperatorError::Model)); clippy fixes (derivable_impls, collapsible_if, useless_format)
    - `layer0/tests/phase1.rs`: `exit_reason_safety_stop_round_trip` serde test; `content_filter_returns_safety_stop` behavioral test
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass, 0 failures); `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` (0 errors)

- 2026-03-03: V32-29 — L5 context introspection query
  - Specs: `specs/04-operator-turn-runtime.md`
  - Adds:
    - `op/neuron-op-react/src/lib.rs`: `ContextSnapshot { messages: Vec<AnnotatedMessage>, token_count: usize, pinned_count: usize, last_compaction_removed: usize }` struct (Debug/Clone/Serialize/Deserialize); `ReactOperator::context_snapshot()` method; `current_context: Arc<Mutex<Vec<AnnotatedMessage>>>` and `last_compaction_removed: Arc<Mutex<usize>>` private fields; 3 update points in execute() (after assembly, after tool results, after compaction); 6 unit tests
    - `op/neuron-op-react/Cargo.toml`: serde dependency added
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass, 0 failures)

- 2026-03-03: V32-30 — MCP tool surface management guidance
  - Specs: `specs/06-composition-factory-and-glue.md`
  - Adds:
    - `turn/neuron-mcp/src/client.rs`: `TOOL_COUNT_WARN_THRESHOLD: usize = 20` pub const; `McpClient::tool_budget_tokens(tools: &[McpTool]) -> usize` associated function (name+desc chars/4 heuristic); `tracing::warn!` in `discover_tools()` when count > threshold; 2 unit tests
    - `turn/neuron-mcp/Cargo.toml`: tracing dependency added; re-exported `TOOL_COUNT_WARN_THRESHOLD` from lib.rs
  - Verify: `nix develop -c cargo test -p neuron-mcp --all-targets` (pass, 31 tests); `nix develop -c cargo clippy --workspace --all-targets -- -D warnings` (0 errors)

## Completed

- 2026-03-03: V32-16 — Expand StoreOptions (Lifetime, ContentKind, salience, ttl)
  - Adds:
    - `layer0/src/state.rs`: `Lifetime { Transient, Session, Durable }`, `ContentKind { Episodic, Semantic, Procedural, Structural, Custom(String) }` enums; `StoreOptions` expanded to 5 optional fields + Serialize/Deserialize
    - `layer0/src/effect.rs`: 4 new fields on `Effect::WriteMemory` (lifetime, content_kind, salience, ttl)
    - All 14 `WriteMemory` constructor sites updated; both effect executors thread new fields into `StoreOptions`
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass)

- 2026-03-03: V32-17 — PreMemoryWrite hook access to StoreOptions metadata
  - Adds:
    - `layer0/src/hook.rs`: `memory_options: Option<StoreOptions>` on `HookContext`
    - `effects/neuron-effects-local`, `orch/neuron-orch-kit`: populate `ctx.memory_options` before PreMemoryWrite dispatch
    - Test: `lifetime_guardrail_blocks_transient_write`
  - Verify: `nix develop -c cargo test -p layer0 -p neuron-effects-local -p neuron-orch-kit --all-targets` (pass)

- 2026-03-03: V32-18 — Lifetime-aware MemoryStore (transient scope enforcement)
  - Adds:
    - `layer0/src/state.rs`: `clear_transient()` default no-op on `StateStore` + `StateReader`; blanket impl forwards
    - `state/neuron-state-memory/src/lib.rs`: separate transient HashMap; `write_hinted()` routes Transient writes; `clear_transient()` clears scratchpad
    - `op/neuron-op-react/src/lib.rs`: `state_reader.clear_transient()` at start of each turn
  - Verify: `nix develop -c cargo test -p layer0 -p neuron-state-memory -p neuron-op-react --all-targets` (pass)

- 2026-03-03: V32-19 — TTL enforcement in FsStore (lazy expiration)
  - Adds:
    - `state/neuron-state-fs/src/lib.rs`: `write_hinted()` writes `{key}_meta.json` sidecar with `expires_at`; `read()` checks expiry lazily (deletes both files if expired); `list()` filters expired entries
  - Verify: `nix develop -c cargo test -p neuron-state-fs --all-targets` (pass)

- 2026-03-03: V32-20 — AnnotatedMessage + CompactionPolicy
  - Adds:
    - `layer0/src/lifecycle.rs`: `CompactionPolicy { Pinned, Normal, CompressFirst, DiscardWhenDone }` enum
    - `turn/neuron-turn/src/context.rs`: `AnnotatedMessage { message, policy, source, salience }` + `From<ProviderMessage>` + `pinned()` + `from_mcp()` constructors
    - `ContextStrategy::compact()` changed to `Vec<AnnotatedMessage> → Vec<AnnotatedMessage>`
    - `SlidingWindow`: pinned messages survive; sliding window applies to normal only
    - `op/neuron-op-react/src/lib.rs`: internal buffer changed to `Vec<AnnotatedMessage>`; wrap/unwrap at provider and context boundaries
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass)

- 2026-03-03: V32-21 — TieredStrategy (zone-partitioned compaction)
  - Adds:
    - `turn/neuron-turn/src/tiered.rs` (new): `TieredStrategy` with Pinned/Active/Summary/Noise zones; `Summariser` trait; `TieredConfig`
    - Pinned messages survive all compaction; noise (DiscardWhenDone/CompressFirst) discarded; one first-generation summary via optional `Summariser`
  - Verify: `nix develop -c cargo test -p neuron-turn --all-targets` (pass)

- 2026-03-03: V32-22 — ContextCommand via SteeringSource
  - Adds:
    - `turn/neuron-turn-kit/src/lib.rs`: `ContextCommand { Pin, DropOldest, SaveSnapshot, LoadSnapshot, ClearWorking }` enum; `SteeringCommand { Message(ProviderMessage), Context(ContextCommand) }` enum; `SteeringSource::drain()` now returns `Vec<SteeringCommand>`
    - `op/neuron-op-react/src/lib.rs`: `apply_context_commands()` implements all 5 variants; context commands bypass PreSteeringInject hook; 8 unit tests
  - Verify: `nix develop -c cargo test -p neuron-op-react --all-targets` (pass)

- 2026-03-03: DX-07 — Developer docs for custom operator
  - Adds:
    - `docs/book/src/guides/custom-operator.md` (new): three-primitive wiring, HookKind composition, SteeringSource observability
    - `docs/book/src/guides/operators.md`: corrected 6-arg `ReactOperator::new` example; link to custom-operator guide
    - `docs/book/src/guides/hooks.md`: HookKind composition rules + steering observability sections
    - `docs/book/src/SUMMARY.md`: Custom Operator entry
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass)

- 2026-03-03: V32-14 — Safety stop reason mapping (Anthropic refusal + OpenAI content_filter)
  - Specs: `specs/04-operator-turn-runtime.md`
  - Adds:
    - `neuron-provider-anthropic`: `"refusal"` → `Ok(ProviderResponse { stop_reason: StopReason::ContentFilter })`
    - `neuron-provider-openai`: `"content_filter"` → `Ok(ProviderResponse { stop_reason: StopReason::ContentFilter })`
  - Verify: `nix develop -c cargo test -p neuron-provider-anthropic -p neuron-provider-openai -p neuron-provider-ollama --all-targets` (pass)

- 2026-03-03: V32-12 — BudgetEvent lifecycle variants (step/loop/time approaching + reached)
  - Specs: `specs/04-operator-turn-runtime.md`, `specs/09-hooks-lifecycle-and-governance.md`
  - Adds:
    - `layer0/src/lifecycle.rs`: 5 new `BudgetEvent` variants: `StepLimitApproaching`, `StepLimitReached`, `LoopDetected`, `TimeoutApproaching`, `TimeoutReached`
    - `op/neuron-op-react/src/lib.rs`: `BudgetEventSink` trait + `with_budget_sink()` builder; approaching events at 80% threshold
    - `layer0/tests/phase1.rs`: 5 round-trip serde tests
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass)

- 2026-03-03: V32-15 — MCP resource/prompt protocol extensions
  - Specs: `specs/07-mcp-tool-integration.md`
  - Adds:
    - `turn/neuron-mcp/src/client.rs`: `McpResourceWrapper`, `McpPromptWrapper`, `discover_resources()`, `discover_prompts()`
    - `turn/neuron-mcp/src/server.rs`: `with_state_reader()`, `with_prompt()` builders; resource/prompt list+read handlers; `state://global/{key}` URIs
    - `turn/neuron-mcp/src/lib.rs`: re-exported `McpResourceWrapper`, `McpPromptWrapper`
  - Verify: `nix develop -c cargo test -p neuron-mcp --all-targets` (pass)

- 2026-03-03: V32-11 — Memory tier metadata on StateStore
  - Specs: `specs/03-effects-and-execution-semantics.md`
  - Adds:
    - `layer0/src/state.rs`: `MemoryTier { Hot, Warm, Cold }`, `StoreOptions`, `read_hinted`/`write_hinted` default methods on `StateStore`/`StateReader`
    - `layer0/src/effect.rs`: `tier: Option<MemoryTier>` on `Effect::WriteMemory`
    - All 11 `WriteMemory` match arms and constructors updated workspace-wide
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass)

- 2026-03-03: V32-13 — CompactionEvent failure/outcome variants
  - Specs: `specs/04-operator-turn-runtime.md`
  - Adds:
    - `layer0/src/lifecycle.rs`: `CompactionFailed`, `CompactionSkipped`, `FlushFailed`, `CompactionQuality` variants
    - `turn/neuron-turn/src/context.rs`: `CompactionError { Transient, Semantic }`; `ContextStrategy::compact()` returns `Result`
    - `op/neuron-op-react/src/lib.rs`: `CompactionEventSink` trait + `with_compaction_sink()` builder; graceful degradation on error
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass)

- 2026-03-03: Provider credential story (ApiKeySource + from_env_var + no-leak tests)
  - Specs: `specs/08-environment-and-credentials.md`, `specs/10-secrets-auth-crypto.md`
  - Adds:
    - `ApiKeySource { Static(String), EnvVar(String) }` crate-private enum in `neuron-provider-anthropic` and `neuron-provider-openai`
    - `AnthropicProvider::from_env_var(var_name)` and `OpenAIProvider::from_env_var(var_name)`: resolve key via `std::env::var` at each `complete()` call
    - `resolve_api_key()` returns `ProviderError::AuthFailed` with var name only — no secret material in error messages
    - 4 unit tests per provider: static-key path, env-var happy path, missing-var `AuthFailed`, empty-var `AuthFailed`, redaction assertion
    - `OllamaProvider` unchanged (no auth required for local inference)
  - Verify: `nix develop -c cargo test -p neuron-provider-anthropic -p neuron-provider-openai --all-targets` (pass, 22 + 24 tests)
  - Commit: `a85b728`

- 2026-03-03: Local effect interpreter and signal/query semantics
  - Specs: `specs/03-effects-and-execution-semantics.md`, `specs/05-orchestration-core.md`
  - Adds:
    - `orch/neuron-orch-kit`: `LocalEffectInterpreter` executes WriteMemory/DeleteMemory against StateStore; maps Delegate/Handoff to followup dispatches; maps Signal to `Orchestrator::signal()`; `OrchestratedRunner` drives depth-first followup queue with safety bound; dispatches `PreMemoryWrite` hook before every write (V32-08 coordinated). 3 effect tests (deterministic order, idempotence, delegate/handoff/signal), 7 runner tests.
    - `orch/neuron-orch-local`: signal journal (`Vec<SignalPayload>` per workflow); `query` returns `{signals: count}`; `signal_count()` getter; 10 tests covering null-workflow (0 count), accept+retrieval, concurrent signals, object safety.
  - Verify: `nix develop -c cargo test -p neuron-orch-kit -p neuron-orch-local --all-targets` (pass)
  - Commit: `eb56e6a` (current HEAD on `feature/gap-analysis-implementation`)

- 2026-03-03: V32 Batch 1 — Hook architecture, retry semantics, exfil guard (V32-02/03/04 layer0/06/07/08 layer0)
  - Specs: `specs/09-hooks-lifecycle-and-governance.md`, `specs/04-operator-turn-runtime.md`
  - Adds:
    - (R1) `layer0/src/hook.rs`: `PreSteeringInject`, `PostSteeringSkip`, `PreMemoryWrite` `HookPoint` variants; `steering_messages`, `skipped_tools`, `memory_key`, `memory_value` fields on `HookContext`
    - (R2) `neuron-hooks`: `HookKind` enum (Guardrail/Transformer/Observer); 3-phase dispatch (observers→transformers→guardrails); `add_guardrail/add_transformer/add_observer` convenience methods; `tracing::warn` on hook errors
    - (R3) `neuron-turn` + all 3 providers: `RequestFailed` split into `TransientError` (retryable), `ContentBlocked` (non-retryable), `AuthFailed` (non-retryable); HTTP 401/403→AuthFailed, content_filter→ContentBlocked, 5xx→TransientError
    - (R4) `neuron-hook-security`: `detect_generic_exfil()` (URL + sensitive data in any tool input); `detect_shell_exfil()` rename; `with_url_pattern()` builder
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass, 0 failures)
  - Commit range: `566282f`–`dc02136`

- 2026-03-03: V32 Batch 2 — Exit priority, steering hooks, compaction reserve, step/loop limits, model selector, PreMemoryWrite wiring (V32-01/04 op-react/05/08 effects/09/10)
  - Specs: `specs/04-operator-turn-runtime.md`, `specs/09-hooks-lifecycle-and-governance.md`, `specs/03-effects-and-execution-semantics.md`
  - Adds:
    - (R5) `neuron-op-react`: ExitCheck hook fires before MaxTurns/BudgetExhausted/Timeout; `poll_steering()` helper dispatches `PreSteeringInject`/`PostSteeringSkip` at all 6 steering sites; `compaction_reserve_pct: f32` (default 0.10) on `ReactConfig`; `max_tool_calls`/`max_repeat_calls` step/loop limits; `model_selector` callback for per-inference routing; `with_model_selector()` builder
    - (R6) `neuron-effects-local` + `neuron-orch-kit`: `LocalEffectExecutor` and `LocalEffectInterpreter` dispatch `PreMemoryWrite` hook before `state.write()`; `with_hooks()` builder on both; Halt blocks write, ModifyToolOutput replaces value
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass, 0 failures)
  - Commit range: `5dd5902`–`97a5f92`

- 2026-03-03: RFC-ExecPrimitives — Factor execution primitives into dedicated crates
  - Specs: `specs/01-architecture-and-layering.md`, RFC: `docs/plans/2026-03-02-execution-primitives-rfc.md`
  - Adds:
    - (A) `neuron-effects-core` crate created (trait + policy), `neuron-effects-local` created; orch-kit re-exports
    - (B) `neuron-turn-kit` crate created (planner/decider/batch-executor/steering traits + BarrierPlanner); `neuron-op-react` depends on turn-kit
  - Verify: `nix develop -c cargo build --workspace` and `nix develop -c cargo test --workspace --all-targets` (pass)

- 2026-02-28: Implement credential resolution + injection + audit story in local mode
  - Specs: `specs/08-environment-and-credentials.md`, `specs/10-secrets-auth-crypto.md`, `specs/09-hooks-lifecycle-and-governance.md`
  - Adds:
    - `neuron-env-local` now supports optional `SecretResolver` wiring and credential injection for `EnvVar`/`File`/`Sidecar` delivery modes
    - `LocalEnv` now emits both `SecretAccessEvent` (audit) and `ObservableEvent` (lifecycle) through a pluggable `EnvironmentEventSink`
    - Credential resolution/injection failures are sanitized to avoid secret-material leakage in `EnvError::CredentialFailed` messages
    - New integration coverage for end-to-end pipeline behavior and no-leak guarantees in `env/neuron-env-local/tests/env.rs`
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass)

- 2026-02-27: Make orchestration "core complete" for composed systems
  - Specs: `specs/03-effects-and-execution-semantics.md`, `specs/05-orchestration-core.md`, `specs/06-composition-factory-and-glue.md`, `specs/11-testing-examples-and-backpressure.md`
  - Adds:
    - `neuron-orch-kit` end-to-end effect pipeline integration test
    - `neuron-orch-local` in-memory workflow signal journal semantics
  - Verify: `nix develop -c cargo test --workspace --all-targets` (pass)

- 2026-02-27: CI hard enforcement (format, tests, clippy) is present
  - Spec: `specs/13-documentation-and-dx-parity.md`
  - Workflow: `.github/workflows/ci.yml`

- 2026-02-27: Root README added (crate map + quickstart)
  - Spec: `specs/13-documentation-and-dx-parity.md`
  - File: `README.md`

- 2026-02-27: Umbrella `neuron` crate added (features + prelude)
  - Spec: `specs/12-packaging-versioning-and-umbrella-crate.md`
  - Crate: `neuron/`

- 2026-03-02: ToolExecutionStrategy added (opt-in)
  - Specs: `specs/04-operator-turn-runtime.md`, `specs/01-architecture-and-layering.md`
  - Adds:
    - `ToolExecutionPlanner` + `BarrierPlanner` and `ConcurrencyDecider`
    - ReactOperator accepts planner/decider; default sequential keeps behavior
    - Shared-batch parallel execution; preserves order and hooks
  - Verify: `nix develop -c cargo test -p neuron-op-react --all-targets` (pass)
- 2026-03-02: SteeringSource (opt-in) and mid-loop injection
  - Specs: `specs/04-operator-turn-runtime.md`, `specs/05-orchestration-core.md`
  - Adds: `SteeringSource` trait; ReactOperator builder; boundary polls with skip semantics; placeholders for skipped tools
  - Verify: `nix develop -c cargo test -p neuron-op-react --all-targets` (pass)
- 2026-03-02: Streaming tool API + ToolExecutionUpdate hook
  - Specs: `specs/09-hooks-lifecycle-and-governance.md`, `specs/04-operator-turn-runtime.md`
  - Adds: `ToolDynStreaming` (optional); `HookPoint::ToolExecutionUpdate`; chunk forwarding in ReactOperator; tests
  - Verify: `nix develop -c cargo test -p neuron-tool -p neuron-op-react --all-targets` (pass)
