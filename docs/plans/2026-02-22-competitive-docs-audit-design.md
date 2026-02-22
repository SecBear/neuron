# Competitive Docs Audit: LLM Discoverability & Agent Comprehensibility

**Date:** 2026-02-22
**Goal:** Unbiased evaluation of neuron vs 4 competitors on how well an LLM
agent can find, understand, and correctly use each library without human help.

## Framing

LLMs work best when types, traits, and field names convey intent without
requiring prose explanation. Rust's type system (named struct fields, exhaustive
enums, trait bounds, doc comments on every public item) provides uniquely high
context density for LLM consumption. Libraries that leverage this advantage
should be dramatically easier for agents to use than stringly-typed or
convention-over-configuration approaches.

Evaluate how much an agent can figure out from type signatures and struct
definitions alone, before reading any docs.

## Competitors

| # | Library | Language | Source |
|---|---------|----------|--------|
| 1 | Rig | Rust | GitHub, crates.io, docs.rs |
| 2 | ADK-Rust | Rust | GitHub (zavora-ai/adk-rust) |
| 3 | Pydantic AI | Python | GitHub, PyPI, docs site |
| 4 | OpenAI Agents SDK | Python | GitHub, PyPI, docs site |
| 5 | neuron (self-audit) | Rust | Local workspace, crates.io |

## Rubric (1-5 per dimension)

### LLM Discoverability (can an agent find it?)

1. **Search presence** -- shows up for "Rust AI agent", "LLM tool use Rust", etc.
2. **Structured metadata** -- llms.txt, crate descriptions, keywords, PyPI classifiers
3. **README first impression** -- instantly communicates what/who/why
4. **docs.rs / docs site quality** -- navigable trait docs, examples, module structure

### Agent Comprehensibility (can an agent use it without help?)

5. **Getting started friction** -- lines/concepts to first working call
6. **Type ergonomics** -- Default::default(), builders, From impls, named fields
7. **Error messages** -- actionable errors an agent can self-correct from
8. **Example coverage** -- real agent patterns (tool use, streaming, multi-turn)
9. **Trait/API clarity** -- small obvious surface vs complex type hierarchy

### Type-Level Context Density (Rust advantage)

10. **Type-level self-documentation** -- how much can an LLM infer from types
    alone? Named struct fields vs positional, exhaustive enums vs strings, trait
    bounds that guide completion, doc comments on every field.

### Polish & Completeness

11. **Feature breadth** -- providers, tool use, streaming, structured output, context
12. **Production readiness** -- retries, timeouts, observability, error recovery
13. **Composability** -- use pieces independently or all-or-nothing?

## Agent Assignments

- **Agents 1-4**: One competitor each, evaluated on full 13-dimension rubric
- **Agent 5**: neuron self-audit using same rubric (does not see competitor results)
- **Agent 6** (sequential, after 1-5 return): Synthesis -- ranked comparison table,
  neuron gaps, actionable recommendations

## Output Format (per agent)

```
## [Library Name] -- Audit

### Scores (1-5)
| Dimension | Score | Notes |

### Strengths
### Weaknesses (what would trip up an agent)
### Notable patterns worth adopting
### Specific quotes (doc text, error messages, code that stood out)
```

## Synthesis Output

- Ranked comparison table (all 13 dimensions x 5 libraries)
- neuron gaps (where we score lower than any competitor)
- Actionable recommendations (specific changes to docs, types, examples, DX)
