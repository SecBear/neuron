⏺ The smoke tests live in agent-blocks/tests/smoke_anthropic.rs and validate
that the entire stack works end-to-end against the real Anthropic API. Here's
what happens in each one, layer by layer.

Setup: Shared helpers

Every test uses these three helpers:

- api_key() — reads ANTHROPIC_API_KEY from the environment, panics if missing
- anthropic() — builds an Anthropic provider pointed at
  claude-haiku-4-5-20251001 (the cheapest model, fractions of a cent per call)
- user_msg(text) — wraps a string into a Message { role: User, content:
  [Text(text)] }

There's also a CalculateTool defined for the tool-use tests — a simple
calculator that can evaluate expressions like "137 * 42" by splitting on the
operator. It implements the Tool trait, which means it has typed args
(CalculateArgs { expression: String }), typed output (CalculateOutput { result:
f64 }), and a JSON Schema generated automatically by schemars.

---

Test 1: smoke_basic_completion

What it proves: Our CompletionRequest serialization and CompletionResponse
deserialization match the Anthropic wire format.

What happens:

1. Builds a CompletionRequest with "What is 2+2? Reply with just the number.",
   temperature: 0.0, max_tokens: 64
2. Calls provider.complete(request) — this is the non-streaming Provider trait
   method. Under the hood, agent-provider-anthropic POSTs to
   https://api.anthropic.com/v1/messages with stream: false, headers x-api-key
   and anthropic-version: 2023-06-01
3. Gets back a CompletionResponse and asserts: - response.id is not empty
   (Anthropic assigned a message ID) - response.model is not empty (confirms
   which model ran) - response.message.role is Assistant -
   response.message.content is non-empty - response.usage.input_tokens > 0 and
   output_tokens > 0 (token counting works) - The first content block is
   ContentBlock::Text containing "4"

If this fails: Either our request JSON doesn't match what Anthropic expects, or
our response parsing is wrong.

---

Test 2: smoke_streaming

What it proves: Our SSE (Server-Sent Events) parser correctly handles
Anthropic's streaming format and produces the right StreamEvent variants.

What happens:

1. Builds a request with "Count from 1 to 5, separated by commas. Nothing else."
2. Calls provider.complete_stream(request) — this POSTs with stream: true.
   Returns a StreamHandle containing a tokio::sync::mpsc::Receiver<StreamEvent>
3. Consumes the stream event by event, collecting into three buckets: -
   TextDelta events — each carries a small chunk of text (e.g., "1", ", ", "2",
   ", 3" etc.) - Usage event — carries final token counts - MessageComplete
   event — carries the fully assembled Message
4. Asserts: - Got at least one TextDelta (streaming actually sent incremental
   chunks) - Got a Usage event with input_tokens > 0 - Got a MessageComplete
   event with role: Assistant - The concatenated text deltas contain both "1"
   and "5"

If this fails: Our SSE line parser, event type mapping, or StreamEvent enum is
wrong. Anthropic sends events like event: content_block_delta with data:
{"type":"content_block_delta","delta":{"type":"text_delta","text":"1"}} — our
streaming module in agent-provider-anthropic/src/streaming.rs parses each line
and maps it.

---

Test 3: smoke_tool_use

What it proves: The full tool-call round trip works — Claude receives a tool
definition, decides to call it, we execute it locally, and send the result back.

What happens in 3 steps:

Step 1 — Ask Claude to use the tool:

1. Builds a request with the CalculateTool's definition included in tools:
   vec![tool_def], tool_choice: Auto
2. Sends "What is 137 * 42? Use the calculate tool."
3. Asserts the response has stop_reason: ToolUse (not EndTurn — Claude stopped
   because it wants to call a tool)
4. Finds a ContentBlock::ToolUse { id, name: "calculate", input } in the
   response content

Step 2 — Execute the tool locally:

1. Calls tool_dyn.call_dyn(input, &ctx) — this goes through the ToolDyn blanket
   implementation which deserializes the JSON input into CalculateArgs, calls
   CalculateTool::call(), and serializes the CalculateOutput back
2. Asserts result.is_error is false and structured_content contains the answer

Step 3 — Send the result back to Claude:

1. Builds a new request with the conversation so far: original user message,
   Claude's assistant message (containing the ToolUse block), and a new user
   message containing ContentBlock::ToolResult { tool_use_id, content, is_error:
   false }
2. Calls provider.complete() again
3. Claude sees the tool result and produces a final text response
4. Asserts the final text contains "5754" or "5,754" (137 × 42 = 5,754)

If this fails: Either our tool definition JSON doesn't match Anthropic's schema,
or the ToolUse/ToolResult content block serialization is wrong, or Claude isn't
recognizing our tool format.

---

Test 4: smoke_full_agent_loop

What it proves: All the crates compose together correctly — AgentLoop
orchestrates the provider, tool registry, and context strategy into a working
agent.

What happens:

1. Creates the three core building blocks: - Provider: Anthropic (real API) -
   Tools: ToolRegistry with CalculateTool registered - Context:
   SlidingWindowStrategy (keeps last 10 messages, up to 100k tokens)
2. Configures LoopConfig with a system prompt ("You are a math assistant. Use
   the calculate tool for any arithmetic.") and max_turns: 5
3. Creates AgentLoop::new(provider, tools, context, config)
4. Calls agent.run(user_msg("What is 99 * 101? Use the calculate tool."), &ctx)

What run() does internally (this is the agentic while loop):

- Turn 1: Sends the user message to Claude → Claude responds with ToolUse {
  name: "calculate", input: {"expression": "99 * 101"} } → Loop detects
  stop_reason: ToolUse → Executes the tool via the registry → Gets ToolOutput {
  result: 9999 } → Appends the tool result as a new message
- Turn 2: Sends the updated conversation to Claude → Claude sees the tool result
  → Responds with text like "99 × 101 = 9,999" → stop_reason: EndTurn → Loop
  terminates

5. Asserts on the AgentResult: - result.turns >= 2 (at least tool call + final
   response) - result.response contains "9999" or "9,999" - result.usage reports
   token counts

If this fails: The composition of agent-loop + agent-tool + agent-context +
agent-provider-anthropic has a bug. Could be context management (messages
getting dropped), tool execution (registry dispatch), or the loop control flow
itself.

---

Cost and safety

All four tests use claude-haiku-4-5-20251001 with small max_tokens (64–256).
Each test costs well under a cent. They're all #[ignore] so they never run in
normal cargo test — only when you explicitly pass -- --ignored.

To run them:

ANTHROPIC_API_KEY=sk-ant-... cargo test -p agent-blocks --test smoke_anthropic
-- --ignored
