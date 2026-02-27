# Neuron Platform Scope Mapping (2026 Agentic Coding “Bible”)

This document maps “agentic coding platform” features (as described in the 2026 agentic coding trends writeup) onto Neuron’s architecture.

Goal: make it obvious which pieces belong **inside Neuron** (stable, reusable kernel) vs which pieces belong in a **platform built on Neuron** (control-plane + SDLC integrations) vs which pieces are simply **external infrastructure** (CI/CD, Git provider, observability stack).

Neuron’s governing intent is in `specs/` (start with `specs/00-vision-and-non-goals.md` and `specs/01-architecture-and-layering.md`). This document is explanatory guidance, not a spec.

## Executive Summary

- Neuron is the **composable kernel**: protocols + effect vocabulary + reference local implementations to prove composition.
- A “go-to agentic platform” is a **product built on Neuron**: orchestration control plane, job scheduler, policy engine, artifact store, dashboards, and SDLC connectors.
- “Context engineering” is a **cross-stack concern**: Neuron provides stable seams (State/Effects/Tools/Hooks/Lifecycle vocab) so a platform can implement rich retrieval/indexing/redaction/budgeting without forking the agent runtime.

## Scope Map (Where Each Feature Lives)

Legend:
- **In Neuron / layer0**: stability contract types + trait boundaries.
- **In Neuron / implementation crates**: local reference implementations and reusable glue (`neuron-orch-kit`, `neuron-op-react`, `neuron-hook-security`, etc.).
- **Outside Neuron (Platform)**: the orchestrator/control-plane product built on Neuron (often a separate repo/service).
- **External infra**: Git provider, CI/CD, ticketing, metrics backends, etc.

### Reference Architecture Components

| Platform feature | In Neuron? | Where it lives (if not) | Why |
|---|---:|---|---|
| Agent Runtime: inner ReAct loop, tool calls, provider calls, budgets, context compaction | Yes | `layer0` + `neuron-op-*` + `neuron-tool` + `neuron-hooks` | This is the reusable “turn” core; different products share it. |
| Orchestrator: dispatch/parallel dispatch + workflow control surfaces (signal/query) | Yes (protocol) / Partial (local impl) | Durable orchestrator: Platform (Temporal/Restate/etc.) | `layer0::Orchestrator` must stay transport-agnostic; durable engines are tech-specific. |
| Context Layer: repo indexing (AST/symbols), semantic search, retrieval pipelines | No (as a subsystem) | Platform services + tools (MCP/local tools) + state backends | Indexing/embeddings are integration-heavy and fast-changing; Neuron provides seams (tools/state/search). |
| Verification Layer: compile/tests/lints/scanners | No (as a runner) | Platform “agent lane” + CI templates + sandbox runner | Verification is environment- and org-specific; Neuron should integrate, not own CI. |
| Delivery Layer: branch/PR creation, review workflows, deploy automation | No | Platform connectors (Git provider, CI/CD, ticketing) | These are SDLC integrations and product UX, not protocol primitives. |
| Observability & Governance: tracing, cost metrics, audit logs, policy decisions | Partial | Platform telemetry + audit stores + policy engine | Neuron provides Hook + lifecycle vocabulary; durable storage/export is platform work. |
| Security-first tool gateway: allowlists, redaction, validation, auditing | Partial | Platform “tool gateway” + sandboxing environment | Neuron provides enforcement seams (Hook points + tool registry); sandbox & egress control are environment/platform. |
| Control plane vs data plane separation | Yes (conceptually) | Platform architecture | Neuron is primarily the data-plane kernel; control-plane scheduling/policy is product-specific. |

## Trend Coverage (From the 2026 Agentic Coding Trends Writeup)

This section answers: “does Neuron cover every piece of the stack described in the article?”

Neuron covers the *kernel primitives* needed to compose the full stack, but many “platform” features are intentionally out of scope and should live in a Neuron-based control plane.

### Trend 1 — Agentic SDLC (state machine + gates)

- **In Neuron**: deterministic gating *seams* (Hooks + effect vocabulary + execution metadata + lifecycle vocab).
- **Outside (Platform)**: the SDLC state machine itself (INTENT→SPEC→PLAN→IMPLEMENT→VERIFY→DOCS→REVIEW→RELEASE→MONITOR), risk scoring, approvals, and CI/CD wiring.

### Trend 2 — Multi-agent coordination (single vs multi-agent architectures)

- **In Neuron**: `dispatch_many`, `Effect::{Delegate,Handoff,Signal}`, and local runnable glue (`neuron-orch-kit`) to prove semantics.
- **Outside (Platform)**: DAG scheduling, specialist routing, artifact store, patch/merge composition, and conflict resolution automation.

### Trend 3 — Long-running agents (durable jobs, days-long work)

- **In Neuron**: stable protocol boundaries that a durable engine can implement (`layer0::Orchestrator`, `layer0::Environment`, `layer0::StateStore`) plus lifecycle vocabulary for budgets/compaction/observability.
- **Outside (Platform)**: durable job records, checkpointing UI, event-sourced logs, workspace snapshotting, retries/circuit breakers, and “reduce scope” policies.

### Trend 4 — Scaled human oversight (selective review)

- **In Neuron**: Hook intervention points (`Halt`, `SkipTool`, `ModifyToolInput`, `ModifyToolOutput`) and standardized per-run metadata (tokens/cost/turns/duration).
- **Outside (Platform)**: risk scoring, escalation policy, review dashboards, and workflow-level approvals.

### Trend 5 — New surfaces and new users (terminal/IDE, ChatOps, web portal)

- **In Neuron**: transport-agnostic tool surfaces (local `ToolDyn` + MCP via `neuron-mcp`) so many frontends can share the same backend capabilities.
- **Outside (Platform)**: the UX surfaces themselves (IDE plugin, Slack bot, web portal) and their identity/permissions integration.

### Trend 6 — Economics and productivity instrumentation

- **In Neuron**: per-operator accounting fields and lifecycle vocabulary for emitting observable events.
- **Outside (Platform)**: metrics pipeline (OpenTelemetry/Prometheus/etc.), cost attribution, ROI dashboards, and budget enforcement policy.

### Trend 7 — Non-technical use cases at scale (enterprise workflow catalog)

- **In Neuron**: safe composition primitives (tools + hooks + effects + environment/state seams).
- **Outside (Platform)**: workflow/template catalog, connector governance, signing/versioning of templates, and promotion/deploy workflows.

### Trend 8 — Dual-use risk and security-first architecture

- **In Neuron**: least-privilege by construction (tools are explicit; side effects are declared as effects), plus hooks for redaction/exfil guards and secret/audit vocabulary.
- **Outside (Platform / Environment)**: sandboxing, network egress allowlists, DLP systems, centralized policy-as-code, and immutable audit log storage.

## “Context Engineering” Across the Stack (How Neuron Enables It)

Neuron enables “intelligent context engineering” by making context a composition problem with explicit seams:

### 1) Persistent context (State)

- Operators **read** prior context/memory via `layer0::StateReader`.
- Operators **write** via declared `Effect::{WriteMemory,DeleteMemory}` (the orchestrator executes these).
- This enforces the rule: *no hidden state; all durability is mediated by orchestration*.

Platform implication: you can build a memory/retrieval system (vector DB, repo index, cache) behind `StateStore::search` and/or tool calls, without changing operators.

### 2) Ephemeral context (Message window + compaction)

- The turn runtime chooses a `ContextStrategy` (e.g. sliding window) to fit prompts into model limits.
- This is where you plug in compaction policies that are cheap (drop old) or smart (summarize).

Platform implication: compaction can be policy-driven (risk/budget dependent) via hooks and lifecycle coordination.

### 3) Retrieved context (Tools)

- Retrieval is naturally expressed as tools (e.g. `repo_search`, `read_file`, “dependency graph”).
- Neuron’s tool abstraction (`ToolDyn`) makes “retrieval + transformation” a first-class plug-in surface.

Platform implication: repo navigators, indexers, and enterprise search should be exposed as tools (often via MCP) and gated by policy (allowlists/denylist + redaction).

### 4) Policy shaping (Hooks)

Hooks can:
- halt execution,
- skip tool calls,
- modify tool input (sanitization),
- modify tool output (redaction).

Platform implication: this is the safest place to implement “tool gateway” semantics without hardcoding policy into every operator.

### 5) Cross-layer coordination (Lifecycle vocabulary)

Neuron defines shared vocabulary (`BudgetEvent`, `CompactionEvent`, `ObservableEvent`, `SecretAccessEvent`) so a platform can:
- correlate turn-level budget usage to workflow-level decisions,
- coordinate compaction across state and turn boundaries,
- produce audit logs and telemetry with stable schemas.

## If It’s Not in Neuron, Where Should It Go?

### Platform repository/service (recommended)

Create a separate “agent platform” that depends on Neuron and owns:
- job schema + state machine + DAG scheduling,
- multi-agent coordination policies,
- SDLC connectors (Git provider, CI, ticketing),
- artifact store (patches, logs, traces),
- dashboards for selective human oversight,
- policy-as-code decisions and approval workflows.

Neuron remains the reusable substrate; the platform is the enterprise product.

### Neuron-adjacent crates (allowed, but keep the boundary)

Some components can be Neuron workspace crates if they are broadly reusable and not org-specific, e.g.:
- additional `Environment` implementations (docker/k8s),
- “repo navigator” tool implementations (read-only by default),
- an OpenTelemetry Hook implementation that exports `ObservableEvent` (pluggable, not mandatory),
- hardened tool gateway helpers built on Hook + ToolDyn (still allowing bypass/escape hatch).

Rule of thumb: if it requires credentials to *your* GitHub org / Jira / production cluster, it’s platform/integration code, not Neuron core.

## Practical Composition Recipe (Use Neuron to Build the Whole Stack)

To compose a “bible-complete” agentic system:

1. Choose operator runtime(s): `neuron-op-react` and/or `neuron-op-single-shot`.
2. Assemble the tool surface: local `ToolDyn` + MCP-imported tools (`neuron-mcp`).
3. Attach governance: `HookRegistry` + security hooks (redaction/exfil) + budget/tool policy hooks.
4. Choose orchestration:
   - local reference: `neuron-orch-local` + `neuron-orch-kit::OrchestratedRunner`
   - durable engine: implement `layer0::Orchestrator` backed by Temporal/Restate/etc.
5. Choose state: `neuron-state-*` implementations; add search/index integrations behind `StateStore::search` and tools.
6. Choose environment: `neuron-env-local` today; add docker/k8s env implementations for true isolation/egress control.
7. Wrap with a platform control plane for SDLC workflows and scaled oversight.
