# Quick Wins Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Improve neuron's getting-started friction, type ergonomics, and docs quality — the six "quick wins" from the competitive audit.

**Architecture:** All changes are additive convenience APIs on existing types. No new crates, no new traits, no behavior changes. Message constructors and ToolContext::default() go in `neuron-types`. Provider `from_env()` goes in each provider crate. Loop uses `..Default::default()`. Trait examples switch from `ignore` to `no_run`.

**Tech Stack:** Rust, serde, tokio_util (CancellationToken), std::env

---

### Task 1: Message convenience constructors

**Files:**
- Modify: `neuron-types/src/types.rs` (after line 131, the Message struct)
- Test: `neuron-types/tests/completion.rs`

**Step 1: Write the failing tests**

Add to `neuron-types/tests/completion.rs`:

```rust
#[test]
fn message_user_constructor() {
    let msg = Message::user("Hello!");
    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content.len(), 1);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello!"));
}

#[test]
fn message_assistant_constructor() {
    let msg = Message::assistant("Hi there");
    assert_eq!(msg.role, Role::Assistant);
    assert_eq!(msg.content.len(), 1);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hi there"));
}

#[test]
fn message_system_constructor() {
    let msg = Message::system("You are helpful");
    assert_eq!(msg.role, Role::System);
    assert_eq!(msg.content.len(), 1);
    assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "You are helpful"));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p neuron-types message_user_constructor message_assistant_constructor message_system_constructor`
Expected: FAIL — no method `user` on `Message`

**Step 3: Write the implementation**

Add an `impl Message` block after the struct definition in `neuron-types/src/types.rs`:

```rust
impl Message {
    /// Create a user message with a single text content block.
    ///
    /// # Example
    ///
    /// ```
    /// use neuron_types::Message;
    /// let msg = Message::user("What is Rust?");
    /// ```
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text(text.into())],
        }
    }

    /// Create an assistant message with a single text content block.
    ///
    /// # Example
    ///
    /// ```
    /// use neuron_types::Message;
    /// let msg = Message::assistant("Rust is a systems programming language.");
    /// ```
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::Text(text.into())],
        }
    }

    /// Create a system message with a single text content block.
    ///
    /// # Example
    ///
    /// ```
    /// use neuron_types::Message;
    /// let msg = Message::system("You are a helpful assistant.");
    /// ```
    #[must_use]
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: vec![ContentBlock::Text(text.into())],
        }
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p neuron-types`
Expected: PASS

**Step 5: Commit**

```bash
git add neuron-types/src/types.rs neuron-types/tests/completion.rs
git commit -m "feat(types): add Message::user(), ::assistant(), ::system() constructors"
```

---

### Task 2: impl Default for ToolContext

**Files:**
- Modify: `neuron-types/src/types.rs` (after the ToolContext struct, ~line 402)
- Test: `neuron-types/tests/completion.rs`

**Step 1: Write the failing test**

Add to `neuron-types/tests/completion.rs`:

```rust
#[test]
fn tool_context_default() {
    let ctx = ToolContext::default();
    assert_eq!(ctx.session_id, "");
    assert!(ctx.environment.is_empty());
    assert!(ctx.progress_reporter.is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p neuron-types tool_context_default`
Expected: FAIL — `Default` not implemented for `ToolContext`

**Step 3: Write the implementation**

Add after the ToolContext struct in `neuron-types/src/types.rs`:

```rust
impl Default for ToolContext {
    /// Creates a ToolContext with sensible defaults:
    /// - `cwd`: current directory (falls back to `/tmp` if unavailable)
    /// - `session_id`: empty string
    /// - `environment`: empty
    /// - `cancellation_token`: new token
    /// - `progress_reporter`: None
    fn default() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp")),
            session_id: String::new(),
            environment: HashMap::new(),
            cancellation_token: CancellationToken::new(),
            progress_reporter: None,
        }
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p neuron-types`
Expected: PASS

**Step 5: Commit**

```bash
git add neuron-types/src/types.rs neuron-types/tests/completion.rs
git commit -m "feat(types): impl Default for ToolContext"
```

---

### Task 3: Use ..Default::default() in loop's CompletionRequest

**Files:**
- Modify: `neuron-loop/src/loop_impl.rs` (~line 260-275)
- Modify: `neuron-loop/src/step.rs` (~lines 138-153, 505-520)

There are 3 locations where CompletionRequest is constructed with all fields
listed explicitly. Replace each with only the meaningful fields +
`..Default::default()`.

**Step 1: No new test needed — existing tests verify behavior**

Run: `cargo test -p neuron-loop`
Expected: PASS (baseline)

**Step 2: Update loop_impl.rs**

Replace the CompletionRequest at ~line 260-275 with:

```rust
let request = CompletionRequest {
    model: String::new(), // Provider decides the model
    messages: self.messages.clone(),
    system: Some(self.config.system_prompt.clone()),
    tools: self.tools.definitions(),
    ..Default::default()
};
```

**Step 3: Update step.rs (first location, ~line 138)**

Replace the CompletionRequest at ~line 138-153 with:

```rust
let request = CompletionRequest {
    model: String::new(),
    messages: self.loop_ref.messages.clone(),
    system: Some(self.loop_ref.config.system_prompt.clone()),
    tools: self.loop_ref.tools.definitions(),
    ..Default::default()
};
```

**Step 4: Update step.rs (second location, ~line 505)**

Replace the CompletionRequest at ~line 505-520 with:

```rust
let request = CompletionRequest {
    model: String::new(),
    messages: self.messages.clone(),
    system: Some(self.config.system_prompt.clone()),
    tools: self.tools.definitions(),
    ..Default::default()
};
```

**Step 5: Run tests to verify nothing broke**

Run: `cargo test -p neuron-loop`
Expected: PASS

**Step 6: Commit**

```bash
git add neuron-loop/src/loop_impl.rs neuron-loop/src/step.rs
git commit -m "refactor(loop): use ..Default::default() for CompletionRequest"
```

---

### Task 4: Change trait doc examples from `ignore` to `no_run`

**Files:**
- Modify: `neuron-types/src/traits.rs` (5 locations: lines 24, 62, 174, 279, 389)

These are skeleton impls with `todo!()` — they compile but panic at runtime.
`no_run` means rustdoc compiles them (catching type errors) but doesn't execute.

**Step 1: No new tests needed — `cargo doc` verifies compilation**

**Step 2: Replace all 5 `ignore` fences with `no_run`**

In `neuron-types/src/traits.rs`, change every occurrence of:
```
/// ```ignore
```
to:
```
/// ```no_run
```

There are exactly 5 occurrences at lines 24, 62, 174, 279, 389.

**Step 3: Verify doc examples compile**

Run: `cargo doc -p neuron-types --no-deps`
Expected: zero warnings, zero errors

If any example fails to compile, fix the example code (add missing imports,
adjust signatures to match current traits).

**Step 4: Commit**

```bash
git add neuron-types/src/traits.rs
git commit -m "docs(types): change trait examples from ignore to no_run"
```

---

### Task 5: Add from_env() to all 3 providers

**Files:**
- Modify: `neuron-provider-anthropic/src/client.rs`
- Modify: `neuron-provider-openai/src/client.rs`
- Modify: `neuron-provider-ollama/src/client.rs`
- Test: `neuron-provider-anthropic/tests/integration.rs`
- Test: `neuron-provider-openai/tests/integration.rs`
- Test: `neuron-provider-ollama/tests/integration.rs`

Each `from_env()` reads the standard env var for that provider's API key and
returns `Result<Self, ProviderError>` (using `ProviderError::Authentication`
for missing var). Ollama's is simpler — no API key needed, just reads optional
`OLLAMA_HOST` for base URL.

**Step 1: Write failing tests**

Add to each provider's `tests/integration.rs`:

**Anthropic:**
```rust
#[test]
fn from_env_missing_key() {
    // Temporarily ensure the var is not set
    std::env::remove_var("ANTHROPIC_API_KEY");
    let result = Anthropic::from_env();
    assert!(result.is_err());
}
```

**OpenAI:**
```rust
#[test]
fn from_env_missing_key() {
    std::env::remove_var("OPENAI_API_KEY");
    let result = OpenAi::from_env();
    assert!(result.is_err());
}
```

**Ollama:**
```rust
#[test]
fn from_env_always_succeeds() {
    // Ollama needs no key — from_env always works
    let client = Ollama::from_env();
    assert!(client.is_ok());
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p neuron-provider-anthropic from_env_missing_key`
Expected: FAIL — no method `from_env`

**Step 3: Implement from_env() on each provider**

**Anthropic** (in `client.rs`, inside `impl Anthropic`):

```rust
/// Create a client from the `ANTHROPIC_API_KEY` environment variable.
///
/// Returns `ProviderError::Authentication` if the variable is not set.
///
/// # Example
///
/// ```no_run
/// use neuron_provider_anthropic::Anthropic;
///
/// let client = Anthropic::from_env()
///     .expect("ANTHROPIC_API_KEY must be set");
/// ```
pub fn from_env() -> Result<Self, ProviderError> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        ProviderError::Authentication(
            "ANTHROPIC_API_KEY environment variable not set".into(),
        )
    })?;
    Ok(Self::new(api_key))
}
```

**OpenAI** (in `client.rs`, inside `impl OpenAi`):

```rust
/// Create a client from the `OPENAI_API_KEY` environment variable.
///
/// Returns `ProviderError::Authentication` if the variable is not set.
/// Also reads `OPENAI_ORG_ID` if present.
///
/// # Example
///
/// ```no_run
/// use neuron_provider_openai::OpenAi;
///
/// let client = OpenAi::from_env()
///     .expect("OPENAI_API_KEY must be set");
/// ```
pub fn from_env() -> Result<Self, ProviderError> {
    let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
        ProviderError::Authentication(
            "OPENAI_API_KEY environment variable not set".into(),
        )
    })?;
    let mut client = Self::new(api_key);
    if let Ok(org) = std::env::var("OPENAI_ORG_ID") {
        client = client.organization(org);
    }
    Ok(client)
}
```

**Ollama** (in `client.rs`, inside `impl Ollama`):

```rust
/// Create a client from environment variables.
///
/// Reads `OLLAMA_HOST` for the base URL if set (e.g. `http://remote:11434`).
/// Always succeeds — Ollama requires no authentication.
///
/// # Example
///
/// ```no_run
/// use neuron_provider_ollama::Ollama;
///
/// let client = Ollama::from_env()
///     .expect("always succeeds");
/// ```
pub fn from_env() -> Result<Self, ProviderError> {
    let mut client = Self::new();
    if let Ok(host) = std::env::var("OLLAMA_HOST") {
        client = client.base_url(host);
    }
    Ok(client)
}
```

**Step 4: Run all provider tests**

Run: `cargo test -p neuron-provider-anthropic -p neuron-provider-openai -p neuron-provider-ollama`
Expected: PASS

**Step 5: Commit**

```bash
git add neuron-provider-anthropic/src/client.rs neuron-provider-openai/src/client.rs neuron-provider-ollama/src/client.rs
git add neuron-provider-anthropic/tests/integration.rs neuron-provider-openai/tests/integration.rs neuron-provider-ollama/tests/integration.rs
git commit -m "feat(providers): add from_env() to Anthropic, OpenAi, and Ollama"
```

---

### Task 6: Simplify README Quick Start

**Files:**
- Modify: `README.md` (root — Quick Start section, lines 20-43)

Now that `Message::user()`, `ToolContext::default()`, and `from_env()` exist,
the Quick Start can be shorter and cleaner. Target: the minimum code a human
or LLM needs to see to understand the library.

**Step 1: Replace the Quick Start**

Replace lines 20-43 of `README.md` with:

```markdown
## Quick Start

```rust,no_run
use neuron::prelude::*;
use neuron::anthropic::Anthropic;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Anthropic::from_env()?;
    let context = SlidingWindowStrategy::new(10, 100_000);

    let mut agent = AgentLoop::builder(provider, context)
        .system_prompt("You are a helpful assistant.")
        .build();

    let result = agent.run_text("Hello!", &ToolContext::default()).await?;
    println!("{}", result.response);
    Ok(())
}
```

Key changes from before:
- `from_env()` instead of hardcoded API key
- `ToolContext::default()` inline instead of separate variable
- Removed explicit `ToolRegistry::new()` and `.tools()` (optional)
- Removed `.max_turns(10)` (has a default)
- 11 lines of Rust instead of 15

**Step 2: Verify it compiles (structure check)**

Run: `cargo build --workspace`
Expected: PASS (README code is `no_run`, not compiled as a doc test unless
it's inside the umbrella crate's lib.rs. Verify manually that types match.)

**Step 3: Commit**

```bash
git add README.md
git commit -m "docs: simplify Quick Start with from_env(), Message::user(), ToolContext::default()"
```

---

### Task 7: Documentation completeness pass

**Files:**
- Modify: `neuron-types/CLAUDE.md` — add Message constructors to key types
- Modify: `neuron-types/README.md` — mention constructors and Default
- Modify: `neuron-provider-anthropic/README.md` — add from_env() example
- Modify: `neuron-provider-openai/README.md` — add from_env() example
- Modify: `neuron-provider-ollama/README.md` — add from_env() example

Follow the doc completeness checklist from root CLAUDE.md. For each change
in Tasks 1-6, ensure all doc surfaces are updated.

**Step 1: Update neuron-types/CLAUDE.md**

Add to Key types:
- `Message::user()`, `::assistant()`, `::system()` — convenience constructors
- `ToolContext` — implements `Default`

**Step 2: Update neuron-types/README.md**

Add Message constructor examples and note that ToolContext implements Default.

**Step 3: Update each provider README**

Add a `from_env()` example to each provider's README, right after the existing
`new()` example.

**Step 4: Verify docs build**

Run: `cargo doc --workspace --no-deps`
Expected: zero warnings

**Step 5: Commit**

```bash
git add neuron-types/CLAUDE.md neuron-types/README.md
git add neuron-provider-anthropic/README.md neuron-provider-openai/README.md neuron-provider-ollama/README.md
git commit -m "docs: update READMEs and CLAUDE.md for quick win changes"
```

---

### Task 8: Final verification

**Step 1:** `cargo build --workspace --examples` — must pass
**Step 2:** `cargo test --workspace` — must pass
**Step 3:** `cargo clippy --workspace --examples` — must pass
**Step 4:** `cargo doc --workspace --no-deps` — zero warnings

If any step fails, fix before proceeding.
