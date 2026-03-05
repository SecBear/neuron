# Rule 10 — Sweep System Backend Separation

## Rationale
The sweep system's `ResearchProvider` trait combines two fundamentally different operations: web research (search) and LLM reasoning (compare, plan). These map to different backends with different auth, costs, rate limits, and failure modes. Conflating them into a single HTTP client is an architectural error.

## Backend Mapping

| `ResearchProvider` method | Backend | Purpose |
|---|---|---|
| `search()` | Parallel.ai Search API | Web research — find evidence |
| `compare()` | LLM (Anthropic Claude) | Structured verdict from evidence |
| `plan()` | LLM (Anthropic Claude) | Generate targeted research queries |

A concrete `ResearchProvider` implementation MUST compose these two backends internally, NOT route everything through one API.

## MUST / MUST NOT
- `search()` MUST call a research/search tool API (Parallel.ai). It MUST NOT call an LLM directly.
- `compare()` and `plan()` MUST call an LLM provider (Anthropic, OpenAI). They MUST NOT route through a research tool API.
- The `ResearchProvider` implementation MUST accept separate credentials for each backend (e.g., `parallel_key_var` and `anthropic_key_var`).
- LLM calls MUST use the Anthropic Messages API directly (reqwest), following the pattern in `neuron-provider-anthropic`. They SHOULD NOT depend on the `Provider` trait (which is not object-safe and would force generics onto `SweepOperator`).
- API keys MUST be resolved from environment variables at call time (never at construction). Error messages MUST contain the variable name only, never key material.

## Anti-patterns
- A `ParallelAiProvider` that sends `compare()` calls to Parallel.ai's task API instead of an LLM.
- A single `api_key_var` field for both research and LLM backends.
- Hardcoding a specific LLM provider — the model identifier should come from `SweepOperatorConfig::model`.

## Examples
- Good: `SweepProvider { parallel_client, parallel_key_var, anthropic_client, anthropic_key_var, model }` with `search()` hitting Parallel.ai and `compare()`/`plan()` hitting Anthropic.
- Bad: `ParallelAiProvider` that routes everything through `https://api.parallel.ai/v1/tasks`.
