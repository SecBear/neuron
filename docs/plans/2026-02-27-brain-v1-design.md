# Brain v1 Design (OrchestratedRunner-first)

Date: 2026-02-27

This document designs Brain v1 (the `brain` POC) per `specs/14-brain-agentic-research-assistant.md`
and `fix_plan.md` item 1.

## Goal

Provide a runnable, offline-testable agentic research assistant that demonstrates Neuron
composition:

- A controller `layer0::Operator` decides what tools to call (ReAct loop).
- Tools include local tools, worker “sub-agent tools”, and MCP-imported tools.
- Side effects are *declared* as `Effect`s by the operator and are *executed* by orchestration glue.
- Runs as a CLI with durable, repo-local state by default.

Non-goals: workflow compiler, patch applier, distributed orchestrator, daemon.

## Approach: OrchestratedRunner-first (Option C)

Brain v1’s executable path uses `neuron-orch-kit::OrchestratedRunner` with:

- `neuron-orch-local::LocalOrch` as the in-process `Orchestrator`
- `neuron-orch-kit::LocalEffectExecutor` to execute effects
- `neuron-state-fs::FsStore` as the default persistent state backend

This makes the “operators declare effects, orchestrator executes effects” boundary real for both
the CLI and integration tests.

## Components

### 1) `brain` crate (library + CLI)

- `brain` crate exposes:
  - `BrainController<P: neuron_turn::provider::Provider>`: builder/wrapper that constructs a
    `neuron-op-react::ReactOperator<P>` with a `ToolRegistry`, hooks, and a state reader.
  - `BrainRuntime`: holds long-lived resources needed by tools (notably MCP clients) and can
    assemble a `ToolRegistry`.
- Binary `brain` provides:
  - `brain run`: run a single prompt through the orchestrated runner, printing the final answer.

### 2) Controller (`ReactOperator`)

The controller is a `neuron-op-react::ReactOperator<P>` configured with:

- `ToolRegistry` assembled by `BrainRuntime`
- `HookRegistry` enabled (initially empty; extension point for policy)
- `StateReader` backed by the chosen `StateStore`
- Per-request `OperatorConfig.allowed_tools` is treated as an allowlist for tool schemas (already
  enforced by `ReactOperator`).

Controller constraints:

- No side effects directly; side effects are represented as `Effect` items in `OperatorOutput.effects`
  (e.g. `write_memory`, `delete_memory`, `delegate`, `handoff`, `signal`).

### 3) State (repo-local by default)

Brain v1 persists effects to filesystem state:

- Default state dir: `./.brain/state` (repo-local, gitignored)
- Configurable via CLI flag `--state-dir` and/or `brain.json`

`OrchestratedRunner` applies memory effects via `LocalEffectExecutor` to `FsStore`.

**Note:** conversation history persistence is *explicit* in v1 (no implicit “write every turn”).

### 4) Tools

Tool surface is assembled into `neuron-tool::ToolRegistry` from:

1. Local Rust tools (enabled/disabled by config).
2. Worker tools (below).
3. MCP-discovered tools (from `.mcp.json`).

Tool gating:

- Global allowlist/denylist (by tool name) filters both:
  - tool schemas exposed to the controller
  - tools actually present in `ToolRegistry`
- Optional tool aliasing via `x-brain` (see below) so controller sees stable names.

### 5) Workers (“sub-agents as tools”)

Workers are `ToolDyn` implementations that call a (usually cheaper) provider/profile and return
structured JSON.

Stable tool ids:

- `sonnet_summarize`
  - input: `{ "text": "...", "goal": "..." }`
  - output: `{ "summary": "...", "key_points": ["..."] }`
- `nano_extract`
  - input: `{ "text": "...", "schema": { ... } }`
  - output: `{ "extracted": { ... } }`
- `codex_generate_patch`
  - input: `{ "task": "...", "context": { ... } }`
  - output: `{ "patch": "...unified diff...", "notes": "..." }`

Implementation notes:

- Worker tools have their own budgets (max turns, max tokens, max cost) independent of controller.
- Worker outputs must be valid JSON; invalid JSON is an error.

### 6) MCP configuration (`.mcp.json`)

Brain accepts Claude/Codex compatible `.mcp.json`:

- Reads `mcpServers` baseline shape and ignores unknown fields.
- Optional Brain extensions live under `x-brain` and must be ignorable by other consumers.

`x-brain` v1:

- `allowlist` / `denylist` by tool name.
- `aliases`: map remote tool names to names exposed in Brain’s `ToolRegistry`.

Brain connects to MCP servers via `neuron-mcp::McpClient`, discovers tools, and registers them.
Brain runtime retains MCP clients for the duration of the run so tool wrappers remain callable.

## Tests & Backpressure

Brain v1 must include an offline integration-style test proving:

1. Controller selects a worker tool.
2. Worker tool calls its provider and returns structured JSON.
3. Controller synthesizes the worker output into the final answer.

Verification command:

- `nix develop -c cargo test`

