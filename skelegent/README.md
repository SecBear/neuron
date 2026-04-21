# skelegent

Composable agentic AI runtime — umbrella crate.

Depend on this crate to get every skelegent component in one place:

```toml
[dependencies]
skelegent = { version = "0.1" }
```

See the workspace root `README.md` for the full architecture overview and
`specs/v2/20-v2-architecture-redesign.md` for the design rationale.

## Usage

```rust,ignore
use skelegent::prelude::*;
use skelegent::anthropic::AnthropicProvider;
use skelegent::builder::agent;

let provider = AnthropicProvider::from_env_var("ANTHROPIC_API_KEY");
let my_agent = agent(provider, my_behaviour)
    .id("coding-agent")
    .name("Coding Agent")
    .tool("bash", bash_tool)
    .tool("read_file", read_file_tool)
    .build();
```

## What's included

- `skelegent::layer0` — protocol traits, wire types, IDs
- `skelegent::context_engine` — Context, AgentLoop, Router, SyncOperator
- `skelegent::turn` — Provider trait, inference types
- `skelegent::anthropic`, `skelegent::openai`, `skelegent::ollama` — providers
- `skelegent::state_memory`, `skelegent::state_fs` — state stores
- `skelegent::compute` — Python execution runtime
- `skelegent::env_local` — local environment provider
- `skelegent::secret`, `skelegent::auth` — secret resolution & auth

## License

MIT OR Apache-2.0
