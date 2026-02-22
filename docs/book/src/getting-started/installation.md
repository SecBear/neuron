# Installation

## Using the Umbrella Crate

The fastest way to get started is the `neuron` umbrella crate with feature flags:

```toml
[dependencies]
neuron = { features = ["anthropic"] }
```

Or install via cargo:

```bash
cargo add neuron --features anthropic
```

## Feature Flags

| Feature | Enables | Default |
|---------|---------|---------|
| `anthropic` | `neuron-provider-anthropic` | Yes |
| `openai` | `neuron-provider-openai` | No |
| `ollama` | `neuron-provider-ollama` | No |
| `mcp` | `neuron-mcp` (Model Context Protocol) | No |
| `runtime` | `neuron-runtime` (sessions, guardrails) | No |
| `full` | All of the above | No |

## Using Individual Crates

Each neuron crate is independently published. Use them directly for finer
control over dependencies:

```toml
[dependencies]
neuron-types = "*"
neuron-provider-openai = "*"
neuron-tool = "*"
neuron-loop = "*"
```

This pulls in only what you need — no transitive dependency on providers you
don't use.

## Minimum Supported Rust Version

neuron requires **Rust 1.90+** (edition 2024). It uses native async traits
(RPITIT) and requires no `#[async_trait]` macro.

## Environment Variables

Each provider loads credentials from environment variables via `from_env()`:

| Provider | Environment Variable |
|----------|---------------------|
| Anthropic | `ANTHROPIC_API_KEY` |
| OpenAI | `OPENAI_API_KEY` |
| Ollama | `OLLAMA_HOST` (default: `http://localhost:11434`) |
