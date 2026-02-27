# Brain (Research Backend)

Brain is a Neuron-based ResearchOps component.

## What Brain Is For

Brain’s job is to produce **grounded research bundles**:

- machine-readable: `index.json`
- human-readable: `findings.md`
- inspectable raw artifacts: `sources/*`, `notes/*`, `tables/*`

These bundles are intended to seed downstream workflows (spec synthesis, implementation factories)
without requiring the consumer to re-scrape or re-discover the underlying evidence.

## What Brain Is Not

Brain is not:

- an interactive IDE harness
- a general workflow engine
- a patch applier or deployment tool

## How It Fits With Claude Code / Codex

The recommended workflow is:

1. Use an interactive harness (Claude Code / Codex) as the **controller**.
2. Brain runs as an **MCP server** providing async research jobs and artifact inspection.
3. The harness uses Brain’s outputs to draft/refine a product spec and proceed to implementation.

Brain can integrate multiple research/scraping backends (Parallel, Firecrawl, BrightData, etc.)
without locking the workspace to a single vendor by importing MCP tools and aliasing them under
canonical roles.

## Specs

- `specs/14-brain-agentic-research-assistant.md` (v1 POC)
- `specs/15-brain-research-backend.md` (v2 ResearchOps backend)

