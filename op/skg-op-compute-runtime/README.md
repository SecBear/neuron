# skg-op-compute-runtime

Programmable execution runtime for Skelegent agents.

This crate is the first proof-of-concept for a **generic compute substrate** inside Skelegent. The public architecture is compute-generic, while the first implementation is Python-specific.

## What it is

This crate adds a reusable execution substrate with these public nouns:

- `ComputeRuntime` — session-scoped programmable execution contract
- `ComputeBackend` — where/how code runs
- `ComputeSession` — session metadata and lifecycle identity
- `ExecutionProfile` — declared environment, working-directory, and session policy
- `SessionPolicy` / `SessionReuseMode` — session reuse, lifetime, reset behavior
- `ExecutionReport` — structured execution result decoupled from transcript updates

The current v0 implementation is:

- a persistent **local Python subprocess worker**
- a generated **core + fs Python prelude**
- an explicit `working_dir` on `ExecutionProfile` so file access is deterministic when the backend supports it
- a `PythonExecTool` (an `Operator`/tool) that executes model-supplied Python code via the runtime and returns the structured result

## What it is not

This crate is **not**:

- a deep research framework
- a search/extract agent
- a notebook system
- a public binding-plugin API yet
- a hardened sandbox backend yet

It is the minimal long-term-form substrate for those things.

## Current file layout

```text
src/
  lib.rs
  backend.rs        # ComputeBackend + request/response types
  error.rs          # ComputeError
  tool.rs           # PythonExecTool — Operator/tool adapter
  profile.rs        # ExecutionProfile + SessionPolicy + SessionReuseMode
  report.rs         # ExecutionReport + ExecutionMetrics
  runtime.rs        # ComputeRuntime + InMemoryComputeRuntime
  session.rs        # ComputeSession

  python/
    mod.rs
    prelude_generator.rs
    worker_protocol.rs
    bridge.rs
    worker.py
```

## End-to-end flow

`PythonExecTool` is an `Operator`; register it in a `Router` so an `AgentLoop`
can call it as the `python_exec` tool. The agent (model) supplies the Python
code in the tool-call input:

```text
agent tool call: python_exec { "code": "...", "session": "..."? }
  -> PythonExecTool
    -> parses input JSON: required `code`, optional `session`
    -> session key = input.session, else the dispatch ID (isolated per dispatch)
    -> calls ComputeRuntime::exec(session_id, code, profile)
      -> InMemoryComputeRuntime resolves/reuses a session
      -> LocalPythonBackend starts or reuses a persistent worker process
      -> worker.py executes code in a shared namespace
      -> generated prelude provides helper functions
      -> worker returns stdout/stderr/final_result/notes
    -> PythonExecTool returns final_result if present, else stdout
```

## Python UX

The model-facing surface is raw Python.

The worker installs a small generated prelude into the interpreter namespace:

- `final(value)`
- `note(text)`
- `capabilities()`
- `help_bindings(module=None, name=None)`
- `read(path, offset=1, limit=None)`
- `write(path, content)`
- `append(path, content)`
- `find(pattern, path='.', hidden=False, limit=1000)`
- `grep(pattern, path, ignore_case=False, literal=False)`

So the model writes normal Python like:

```python
x = 40
y = 2
note("computed answer")
final({"answer": x + y})
```

Not JSON-ish tool payloads.
Not file-write plus bash indirection.

The tool's capability descriptor documents that these helpers exist; `capabilities()` / `help_bindings()` are the in-session discovery path for built-ins.

## Sessions

`InMemoryComputeRuntime` manages sessions by `SessionId`.

### Current behavior
- `SessionReuseMode::Reuse`
  - persistent session reused across execs
  - session rejects profile changes
  - session can reset on error
  - session expires on idle timeout or max lifetime
- `SessionReuseMode::Fresh`
  - every exec starts/stops a fresh backend handle
  - nothing is retained

### Reset / close
- `reset(session)` recreates the backend handle for the same session id
- `close(session)` terminates the session and removes it from the session map

### Working directory
- `ExecutionProfile.working_dir` lets callers pin the backend cwd explicitly
- the local Python backend applies it at worker startup
- fs helpers operate relative to that directory when paths are relative

## Backend

The current backend is `python::LocalPythonBackend`.

### Current transport
- stdio
- 4-byte big-endian length-prefixed JSON messages
- `init`, `exec`, `reset`, `close`
- worker startup honors `ExecutionProfile.working_dir` when present

### Worker responsibilities
`python/worker.py`:
- maintains a persistent namespace
- reinstalls the prelude on reset
- captures stdout/stderr
- stores structured result in `__SKG_RESULT`
- returns:
  - `stdout`
  - `stderr`
  - `exit_code`
  - `final_result`
  - `notes`

## Capability discovery

The Python prelude includes capability discovery in v0:

- `capabilities()` returns the projected capability descriptors for the active prelude modules
- `help_bindings()` returns human-readable summaries, with optional module/name filtering

This keeps the runtime self-describing and avoids overloading the prompt with all future capability text.

## Current tests

This crate currently has two test groups:

### `tests/runtime.rs`
Locks the public compute nouns and session behavior:
- profile reuses `EnvironmentSpec`
- session defaults
- session reuse
- reset behavior
- close behavior
- reset-on-error
- non-zero exit handling
- session expiry
- profile mismatch rejection

### `tests/python_worker.rs`
Locks the Python-specific substrate:
- prelude exposes the core and fs functions
- capability projection matches the generated payload
- direct worker protocol round-trip works
- namespace persists across execs
- reset clears namespace
- backend exec respects `working_dir` and fs helpers

### `PythonExecTool`
No dedicated test yet — gap. The tool's input parsing and result mapping are
exercised only indirectly. (TODO: add `tests/tool.rs`.)

## Usage

The standalone `examples/compute-python-poc` binary was removed in the v2
redesign. Use `PythonExecTool` as a tool inside an agent: construct it with
`PythonExecTool::new(runtime, profile)` (a `ComputeRuntime` such as
`InMemoryComputeRuntime`, plus an `ExecutionProfile`), register it in a
`Router`, and drive it from an `AgentLoop`. The agent calls it as the
`python_exec` tool with `{ "code": "...", "session": "..."? }`.

## Ergonomic issues / signals from the PoC

Implementing this PoC surfaced a few useful signals about Skelegent core design:

### 1. `EnvironmentSpec` reuse is good
Using `EnvironmentSpec` inside `ExecutionProfile` felt correct. We did not need a parallel environment-policy model.

### 2. The current `Environment` trait is too coarse for sessioned interpreters
`Environment::run(ctx, input, spec)` is a one-shot operator execution boundary. It is not the right trait for persistent interpreter sessions. This suggests Skelegent may eventually want a lower-level reusable execution backend substrate below both `Environment` and `ComputeBackend`.

### 3. Structured execution output matters
We had to extend `BackendExecResponse` with `final_result` and `notes`. Raw stdout/stderr alone is not enough for a serious programmable substrate. That is a strong signal that code execution should stay a first-class runtime concept, not collapse into ordinary tools.

### 4. Session identity fallback is awkward
The operator currently falls back to `SessionId::new(format!("compute-{}", ctx.dispatch_id.as_str()))` when `OperatorInput.session` is absent. This is honest for a PoC, but it suggests future builders may want a more explicit, ergonomic session-selection policy or helper.

### 5. Binding/prelude machinery should stay internal until proven
The internal binding definitions, capability projection, and prelude generation are useful, but exposing them publicly now would freeze the wrong API too early.

### 6. The backend/session split feels right
The `ComputeRuntime` / `ComputeBackend` split held up well. The local subprocess backend can later be swapped for Docker, bubblewrap/Nix, microVM, or remote workers without changing the model-facing Python UX.

## What is intentionally deferred

Not in this PoC yet:

- web/state/dispatch binding modules
- Nix / bubblewrap backend
- Docker backend
- microVM / remote backend
- public binding plugin API
- public language adapter abstraction
- notebook/display-rich execution

## Branch

This PoC lives on the feature branch:

- `feat/per-196-compute-runtime-python-poc`

Based on:

- `v2`

## Status

The crate is usable as a PoC today, but still intentionally narrow. It proves:

- generic compute-runtime nouns
- persistent Python execution
- composable core + fs prelude generation
- capability discovery in-session
- explicit working-directory control for file-oriented compute tasks
- structured final result return
- separation between user-facing operator and compute substrate
