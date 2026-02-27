# Brain: Agentic Research Assistant (POC Spec)

This spec defines a small agentic system named `brain` that showcases Neuron's composability.

`brain` is intentionally not a general workflow product. It is a research/planning assistant
whose job is to translate user intent into tool calls, synthesize the results, and return a
high-effort answer. Producing downstream implementation artifacts (spec creation, patch
application, deployment) is out of scope.

## Goals

`brain` MUST:

1. Use a "smart" user-facing controller model (Anthropic Opus or OpenAI GPT-5.x class models)
   to decide which tools to call and when to stop.
2. Support "worker" sub-agents as tools that call cheaper models to do bounded sub-tasks
   (summarize, extract, search, generate patch candidates).
3. Allow tools to be attached and removed without changing `brain` source code.
4. Allow local tools to be disabled, and enforce allowlists/denylists of tool names.
5. Provide a stable configuration story for MCP that is compatible with Claude Code and Codex,
   with Neuron-specific extensions only when required.

`brain` SHOULD:

1. Be fully testable without network access by using mock providers and mock MCP tools.
2. Provide at least one runnable example that proves the end-to-end loop:
   user intent -> tool calls -> synthesis -> final answer.

## Non-Goals

`brain` is NOT:

1. A workflow compiler.
2. A distributed orchestrator.
3. A patch applier, repo writer, or deployer.
4. A long-running daemon (v1 can be a CLI).

## Architecture

### Controller

`brain` v1 controller is a Neuron-native `layer0::Operator` implementation:

1. The controller MUST be implemented using `neuron-op-react::ReactOperator<P>`,
   where `P` is a `neuron-turn::provider::Provider` (OpenAI/Anthropic).
2. The controller MUST use a `ToolRegistry` that contains:
   - Imported MCP tools (from `.mcp.json` config).
   - Optional local tools (compile-time implementations) that are toggled by config.
   - Worker tools (below).
3. The controller MUST have hooks enabled via `HookRegistry` so policy can halt or modify tool calls.
4. The controller MUST accept per-request `OperatorConfig.allowed_tools` and treat it as an allowlist
   that filters the tool schemas exposed to the provider.

Controller behavior constraints:

1. The controller MUST NOT perform side effects directly.
2. Side effects MUST be represented as `Effect` items in `OperatorOutput.effects` (e.g. write_memory).
3. The controller MUST use tools for all "doing", including repo search and web search.

### Workers (Sub-Agents as Tools)

Workers are tools that perform bounded work and return structured JSON.

In v1, a worker is implemented as `Arc<dyn ToolDyn>` with `call(input_json) -> output_json`.

Worker semantics:

1. A worker tool MUST call a model provider (often cheaper than the controller).
2. A worker tool MAY run its own small ReAct loop using `ReactOperator` internally, with its own
   `ToolRegistry` (typically a subset of tools).
3. A worker tool MUST have explicit budgets (max turns, max tokens, max cost) that are independent
   of the controller budgets.
4. A worker tool MUST return a machine-readable JSON shape (not only raw text) so the controller
   can reliably synthesize.

Required worker tools (names are stable tool ids):

1. `sonnet_summarize`:
   - Input: `{ "text": "...", "goal": "..." }`
   - Output: `{ "summary": "...", "key_points": ["..."] }`
2. `nano_extract`:
   - Input: `{ "text": "...", "schema": { ...json schema... } }`
   - Output: `{ "extracted": { ... } }`
3. `codex_generate_patch` (name is conceptual; provider is configurable):
   - Input: `{ "task": "...", "context": { ... } }`
   - Output: `{ "patch": "...unified diff...", "notes": "..." }`

Note: tool naming is about role, not vendor. For example, `sonnet_summarize` could be implemented
with any "cheap summarizer" profile; the name communicates intent to the controller.

### Tool Runtime

The tool runtime is `neuron-tool::ToolRegistry`.

Tool sources:

1. Local Rust `ToolDyn` implementations registered by `brain`.
2. MCP imported tools:
   - `brain` MUST connect to one or more MCP servers using `neuron-mcp::McpClient`.
   - `brain` MUST discover tools and register them in `ToolRegistry`.

Tool gating:

1. `brain` MUST support a global allowlist/denylist (by tool name) that applies to:
   - tools exposed to the controller model (tool schema list)
   - tools actually executable in the registry
2. `brain` MUST support disabling local tools.

### Orchestration

`brain` v1 does not require a distributed orchestrator. It is a single CLI invocation.

However, `brain` SHOULD be runnable through `neuron-orch-kit` to prove effects are executable:

1. Register the controller as agent `brain`.
2. Use `OrchestratedRunner<LocalEffectExecutor<_>>` so memory effects are executed against a StateStore.

This is a composability proof, not a product requirement.

## Configuration

### brain.json

`brain.json` is the primary config. It defines:

1. Controller provider selection and model id.
2. Worker profiles (provider/model/budgets).
3. Tool assembly policy:
   - which `.mcp.json` files to load
   - local tools enabled/disabled
   - allowlist/denylist

The exact JSON schema for `brain.json` is not yet standardized; v1 should be pragmatic and stable.

### .mcp.json (Claude/Codex-Compatible)

`.mcp.json` defines MCP server connections in a Claude Code / Codex compatible shape.

Compatibility rules:

1. `brain` MUST accept the baseline Claude/Codex `mcpServers` structure and ignore unknown fields.
2. If `brain` needs additional metadata, it MUST be added under a namespaced extension key:
   `x-brain`.
3. Claude/Codex SHOULD be able to consume the file even if `x-brain` is present (they ignore it).

`x-brain` MAY contain:

1. Tool allowlist/denylist.
2. Tool aliases (rename tools for controller prompt ergonomics).
3. Tool grouping into named toolsets.

## Tests and Examples

`brain` MUST have:

1. Offline tests that do not require network access:
   - controller uses a `MockProvider`
   - worker tools use `MockProvider`
   - MCP tools are provided by a local in-process registry or a fake MCP server
2. At least one integration-style test that proves:
   - the controller selects a worker tool
   - the worker tool calls its provider and returns structured JSON
   - the controller synthesizes that result into the final answer

## Brain v2 (External Harness Mode)

Brain v2 adds an optional mode where the controller loop is hosted by an external harness
(Claude Code / Codex) while `brain` hosts tools.

For the v2 ResearchOps “async jobs + grounded bundles” direction, see:

- `specs/15-brain-research-backend.md`

Requirements:

1. `brain` MUST be able to run as an MCP server (`neuron-mcp::McpServer`) exposing the same tool surface.
2. `brain` SHOULD provide configuration passthrough for harness behavior via CLI args/env vars
   (best-effort; enforcement is weaker than Neuron-native mode).
3. Brain v2 MUST preserve the "tool surface contract" so behavior is comparable to v1, even if
   internal policy enforcement differs.
