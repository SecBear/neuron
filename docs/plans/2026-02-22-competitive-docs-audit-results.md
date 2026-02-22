# Competitive Docs Audit Results

**Date:** 2026-02-22
**Methodology:** 5 independent agents evaluated libraries on a 13-dimension rubric
(LLM discoverability, agent comprehensibility, type-level context density, polish).
A 6th synthesis agent cross-compared all findings.

## Ranked Comparison Table

| # | Dimension | Pydantic AI | OpenAI Agents | neuron | Rig | ADK-Rust |
|---|-----------|:-----------:|:-------------:|:------:|:---:|:--------:|
| 1 | Search presence | **5** | **5** | 2 | **5** | 3 |
| 2 | Structured metadata | **5** | 3 | 4 | 3 | 3 |
| 3 | README first impression | 4 | **5** | 4 | 4 | 4 |
| 4 | Docs quality | **5** | **5** | 3 | 2 | 2 |
| 5 | Getting started friction | **5** | **5** | 3 | 4 | 3 |
| 6 | Type ergonomics | **5** | 4 | 4 | 3 | 3 |
| 7 | Error messages | 4 | 4 | **4** | 3 | 2 |
| 8 | Example coverage | 5 | 5 | 4 | **5** | **5** |
| 9 | Trait/API clarity | 4 | 4 | **5** | 3 | 3 |
| 10 | Type-level self-doc | **5** | 4 | **5** | 3 | 3 |
| 11 | Feature breadth | 5 | 5 | 4 | **5** | **5** |
| 12 | Production readiness | **5** | 4 | 2 | 3 | 3 |
| 13 | Composability | 4 | 3 | **5** | 2 | 3 |
| | **Total** | **61** | **56** | **49** | **45** | **42** |

neuron is the highest-scoring Rust library. It leads on trait/API clarity (5),
type-level self-documentation (5), and composability (5). It trails on search
presence (2), production readiness (2), and getting-started friction (3).

## neuron Gaps (where we score below any competitor)

### Dim 1: Search presence (2 vs everyone's 3-5)
"neuron" collides with neuroscience. No blog posts, conference talks, or
third-party mentions. llms.txt helps LLM-mediated discovery but not human search.

### Dim 3: README first impression (4 vs OpenAI Agents' 5)
Quick Start is 18 lines vs OpenAI's 4. Requires constructing ToolContext with
5 fields. Root README implies ToolContext::default() exists -- it doesn't.

### Dim 4: Docs quality (3 vs Pydantic AI/OpenAI Agents' 5)
No standalone docs site. Trait examples use `ignore` (not compile-checked).
No cross-linking between crates on docs.rs.

### Dim 5: Getting started friction (3 vs Pydantic AI/OpenAI Agents' 5, Rig's 4)
No `from_env()` on providers. No ToolContext::default(). Message construction
is 3 lines for what should be 1.

### Dim 8: Example coverage (4 vs everyone's 5)
17 examples vs Rig's 90+, ADK-Rust's 120+, OpenAI's 134. Missing: streaming
agent loop, MCP server, testing guide, error handling patterns, HITL approval.

### Dim 11: Feature breadth (4 vs everyone's 5)
3 providers vs Rig's 20+. No embeddings. No resilience layer. No OpenAI-compat
thin wrappers for Groq/DeepSeek/Together.

### Dim 12: Production readiness (2 vs Pydantic AI's 5, everyone else's 3-4)
Weakest dimension. No retry/backoff. No rate limiter. No concrete
ObservabilityHook. DurableContext trait exists but only LocalDurableContext
(passthrough) ships. Loop doesn't use ..Default::default() for CompletionRequest.

## Actionable Recommendations

### Quick Wins (< 1 hour each)

| ID | Change | Impact |
|----|--------|--------|
| QW-1 | `Message::user()`, `::assistant()`, `::system()` constructors | Dim 5, 6 |
| QW-2 | `impl Default for ToolContext` | Dim 3, 5 |
| QW-3 | Use `..Default::default()` in loop's CompletionRequest | Dim 12 |
| QW-4 | Change trait doc examples from `ignore` to `no_run` | Dim 4 |
| QW-5 | Add `from_env()` to all 3 providers | Dim 5 |
| QW-6 | Simplify README Quick Start (after QW-1/2/5) | Dim 3, 5 |

### Medium Effort (1 day each)

| ID | Change | Impact |
|----|--------|--------|
| ME-1 | Ship a `TracingHook` (concrete ObservabilityHook) | Dim 12 |
| ME-2 | Add retry-with-backoff in the agent loop | Dim 12 |
| ME-3 | Add 10+ focused examples covering gaps | Dim 8 |
| ME-4 | Feature `#[neuron_tool]` macro in Quick Start path | Dim 5, 8 |
| ME-5 | Add crates.io categories to all Cargo.toml files | Dim 1, 2 |

### Larger Investments (multi-day)

| ID | Change | Impact |
|----|--------|--------|
| LI-1 | OpenAI-compat providers (Groq, DeepSeek, Together) | Dim 11 |
| LI-2 | Standalone docs site (mdbook or similar) | Dim 4 |
| LI-3 | Ship a real LocalDurableContext with file journaling | Dim 12 |
| LI-4 | Search-friendly branding / landing page | Dim 1 |

The six quick wins can be done in a single afternoon and would move neuron's
score from ~49 to ~53-54. Medium-effort items would reach ~57-58. Larger
investments are needed to compete with Pydantic AI's 61.

## Patterns Worth Adopting

From competitors, these stood out:

- **Pydantic AI**: `Agent[Deps, Output]` typed generics, `Agent.override()`
  context manager for testing, `ToolPrepareFunc` for conditional tool
  registration, `DeferredToolRequests` for HITL
- **OpenAI Agents**: 5-step agent loop explanation in README, `ModelBehaviorError`
  vs `UserError` distinction, `StopAtTools` for declarative loop termination
- **Rig**: Distributable AI skill packages for every coding agent, `Agent`
  implements `Tool` for multi-agent composition, `from_env()` on all clients
- **ADK-Rust**: `ResolvedContext` eliminating phantom tool hallucinations,
  `enhanced_description()` for long-running tools, AGENTS.md for AI context
