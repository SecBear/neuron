# neuron-state-graph Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement `neuron-state-graph`, a `StateStore` backed by CozoDB with Hybrid GraphRAG retrieval (vector + BM25 + graph expansion + RRF/MMR ranking).

**Architecture:** CozoDB embedded engine handles storage, HNSW vector search, FTS, and graph traversal via Datalog. A typed Rust wrapper (`CozoEngine`) replaces string-based API with safe builders. `GraphStore` implements `StateStore` for CRUD and a new `GraphStateStore` extension trait for explicit graph operations. Retrieval splits: CozoDB generates candidates, Rust performs RRF fusion + MMR diversity filtering.

**Tech Stack:** Rust, CozoDB v0.7.6 (cozo crate with storage-sqlite feature), layer0 traits, async-trait, serde, tokio

**Design Doc:** `docs/plans/2026-02-28-agent-memory-graphrag-design.md`

---

## Phase 1: Crate Skeleton + Schema Types

### Task 1: Create crate and add to workspace

**Files:**
- Create: `neuron-state-graph/Cargo.toml`
- Create: `neuron-state-graph/src/lib.rs`
- Modify: `Cargo.toml` (workspace root — add member)

**Step 1: Create directory and Cargo.toml**

```bash
mkdir -p neuron-state-graph/src
```

```toml
# neuron-state-graph/Cargo.toml
[package]
name = "neuron-state-graph"
version = "0.1.0"
edition.workspace = true
description = "CozoDB-backed StateStore with Hybrid GraphRAG retrieval for neuron"
license.workspace = true
repository.workspace = true

[dependencies]
layer0 = { path = "../layer0" }
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["sync"] }
cozo = { version = "0.7", features = ["storage-sqlite"] }
uuid = { version = "1", features = ["v4"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

**Step 2: Create lib.rs with module declarations**

```rust
// neuron-state-graph/src/lib.rs
#![deny(missing_docs)]
//! CozoDB-backed StateStore with Hybrid GraphRAG retrieval.
//!
//! Implements layer0's `StateStore` trait with a graph-structured
//! knowledge store using CozoDB's Datalog engine, HNSW vector
//! indices, and full-text search.

pub mod schema;
pub mod scope;
```

**Step 3: Add to workspace**

In root `Cargo.toml`, add `"neuron-state-graph"` to `[workspace] members` list.

**Step 4: Verify it compiles**

Run: `nix develop -c cargo check -p neuron-state-graph`
Expected: Success (empty crate compiles)

**Step 5: Commit**

```bash
git add neuron-state-graph/ Cargo.toml
git commit -m "feat(state-graph): scaffold neuron-state-graph crate"
```

---

### Task 2: Define Layer and NodeType enums

**Files:**
- Create: `neuron-state-graph/src/schema/mod.rs`
- Create: `neuron-state-graph/src/schema/layer.rs`
- Create: `neuron-state-graph/src/schema/node_type.rs`

**Step 1: Write tests for Layer enum**

```rust
// neuron-state-graph/src/schema/layer.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_serialization_roundtrip() {
        for layer in [
            Layer::Reality,
            Layer::Epistemic,
            Layer::Intent,
            Layer::Memory,
            Layer::Agent,
            Layer::Custom("test".into()),
        ] {
            let json = serde_json::to_string(&layer).unwrap();
            let back: Layer = serde_json::from_str(&json).unwrap();
            assert_eq!(layer, back);
        }
    }

    #[test]
    fn layer_display() {
        assert_eq!(Layer::Reality.as_str(), "reality");
        assert_eq!(Layer::Custom("x".into()).as_str(), "custom:x");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `nix develop -c cargo test -p neuron-state-graph`
Expected: FAIL — module not found

**Step 3: Implement Layer enum**

```rust
// neuron-state-graph/src/schema/layer.rs
//! Memory graph layers — organizing knowledge by cognitive function.

use serde::{Deserialize, Serialize};

/// The layer a node belongs to. Layers organize the knowledge graph
/// by cognitive function, from raw observations to agent coordination.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// Raw observations and sources — what research agents find.
    Reality,
    /// Reasoning and knowledge — what agents know and reason about.
    Epistemic,
    /// Goals and decisions — what gets built and why.
    Intent,
    /// Persistence and recall — cross-session continuity.
    Memory,
    /// Control plane — multi-agent coordination.
    Agent,
    /// User-defined layers.
    Custom(String),
}

impl Layer {
    /// String representation for storage.
    pub fn as_str(&self) -> String {
        match self {
            Self::Reality => "reality".into(),
            Self::Epistemic => "epistemic".into(),
            Self::Intent => "intent".into(),
            Self::Memory => "memory".into(),
            Self::Agent => "agent".into(),
            Self::Custom(s) => format!("custom:{s}"),
        }
    }
}
```

**Step 4: Implement NodeType enum**

```rust
// neuron-state-graph/src/schema/node_type.rs
//! Node types in the memory graph.

use super::layer::Layer;
use serde::{Deserialize, Serialize};

/// The type of a node in the memory graph. Each type belongs to a layer.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    // -- Reality layer --
    /// An external information source (URL, document, API).
    Source,
    /// A named entity extracted from sources.
    Entity,
    /// A text snippet extracted from a source.
    Snippet,
    /// A raw observation or data point.
    Observation,

    // -- Epistemic layer --
    /// A factual assertion with confidence.
    Claim,
    /// Supporting or refuting evidence for a claim.
    Evidence,
    /// An abstract concept or category.
    Concept,
    /// A testable hypothesis.
    Hypothesis,
    /// A recurring pattern identified across observations.
    Pattern,
    /// An open question to investigate.
    Question,
    /// A mental or computational model.
    Model,

    // -- Intent layer --
    /// A desired outcome.
    Goal,
    /// A choice made between options.
    Decision,
    /// A scoped unit of work.
    Project,
    /// A specification document for builders.
    Spec,

    // -- Memory layer --
    /// A conversation or interaction session.
    Session,
    /// A compressed summary of other nodes.
    Summary,
    /// A learned user or system preference.
    Preference,

    // -- Agent layer --
    /// An agent identity.
    AgentNode,
    /// A unit of work assigned to an agent.
    Task,
    /// A sequence of steps to accomplish a goal.
    Plan,

    // -- Escape hatch --
    /// User-defined node type.
    Custom(String),
}

impl NodeType {
    /// The layer this node type belongs to.
    pub fn layer(&self) -> Layer {
        match self {
            Self::Source | Self::Entity | Self::Snippet | Self::Observation => Layer::Reality,
            Self::Claim | Self::Evidence | Self::Concept | Self::Hypothesis
            | Self::Pattern | Self::Question | Self::Model => Layer::Epistemic,
            Self::Goal | Self::Decision | Self::Project | Self::Spec => Layer::Intent,
            Self::Session | Self::Summary | Self::Preference => Layer::Memory,
            Self::AgentNode | Self::Task | Self::Plan => Layer::Agent,
            Self::Custom(_) => Layer::Custom("custom".into()),
        }
    }

    /// String representation for storage.
    pub fn as_str(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{self:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_type_layer_mapping() {
        assert_eq!(NodeType::Source.layer(), Layer::Reality);
        assert_eq!(NodeType::Claim.layer(), Layer::Epistemic);
        assert_eq!(NodeType::Goal.layer(), Layer::Intent);
        assert_eq!(NodeType::Session.layer(), Layer::Memory);
        assert_eq!(NodeType::Task.layer(), Layer::Agent);
    }

    #[test]
    fn node_type_serialization_roundtrip() {
        for nt in [NodeType::Source, NodeType::Claim, NodeType::Custom("x".into())] {
            let json = serde_json::to_string(&nt).unwrap();
            let back: NodeType = serde_json::from_str(&json).unwrap();
            assert_eq!(nt, back);
        }
    }
}
```

**Step 5: Wire up schema module**

```rust
// neuron-state-graph/src/schema/mod.rs
//! Schema types for the memory graph.

pub mod layer;
pub mod node_type;

pub use layer::Layer;
pub use node_type::NodeType;
```

**Step 6: Run tests**

Run: `nix develop -c cargo test -p neuron-state-graph`
Expected: All tests pass

**Step 7: Commit**

```bash
git add neuron-state-graph/src/schema/
git commit -m "feat(state-graph): add Layer and NodeType enums"
```

---

### Task 3: Define EdgeType enum

**Files:**
- Create: `neuron-state-graph/src/schema/edge_type.rs`
- Modify: `neuron-state-graph/src/schema/mod.rs`

**Step 1: Write tests**

```rust
// In edge_type.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_type_serialization_roundtrip() {
        for et in [
            EdgeType::Supports, EdgeType::Refutes,
            EdgeType::DependsOn, EdgeType::Custom("x".into()),
        ] {
            let json = serde_json::to_string(&et).unwrap();
            let back: EdgeType = serde_json::from_str(&json).unwrap();
            assert_eq!(et, back);
        }
    }
}
```

**Step 2: Implement EdgeType**

```rust
// neuron-state-graph/src/schema/edge_type.rs
//! Edge types in the memory graph.

use serde::{Deserialize, Serialize};

/// The type of an edge (relationship) between nodes.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    // Structural
    /// Extracted from a source node.
    ExtractedFrom,
    /// Part of a larger whole.
    PartOf,
    /// Derived from another node.
    DerivedFrom,
    /// Contains another node.
    Contains,

    // Epistemic
    /// Supports a claim.
    Supports,
    /// Refutes a claim.
    Refutes,
    /// Contradicts another node.
    Contradicts,
    /// Depends on another node.
    DependsOn,
    /// Relates to another node (general).
    RelatesTo,

    // Intent
    /// Decomposes into sub-goals or sub-tasks.
    DecomposesInto,
    /// Motivated by a goal or decision.
    MotivatedBy,
    /// Blocks progress on another node.
    Blocks,
    /// Informs a decision or goal.
    Informs,
    /// Targets a specific outcome.
    Targets,

    // Memory
    /// Captured in a session.
    CapturedIn,
    /// Summarizes other nodes.
    Summarizes,
    /// Recalls a previous node.
    Recalls,

    // Agent
    /// Assigned to an agent.
    AssignedTo,
    /// Planned by a plan node.
    PlannedBy,
    /// Produced by an agent or process.
    ProducedBy,

    // Temporal
    /// Superseded by a newer version.
    SupersededBy,
    /// Invalidated at a point in time.
    InvalidatedAt,

    /// User-defined edge type.
    Custom(String),
}

impl EdgeType {
    /// String representation for storage.
    pub fn as_str(&self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{self:?}"))
    }
}
```

**Step 3: Add to schema/mod.rs**

Add `pub mod edge_type;` and `pub use edge_type::EdgeType;`.

**Step 4: Run tests**

Run: `nix develop -c cargo test -p neuron-state-graph`
Expected: All pass

**Step 5: Commit**

```bash
git add neuron-state-graph/src/schema/
git commit -m "feat(state-graph): add EdgeType enum"
```

---

### Task 4: Define GraphNode and GraphEdge structs

**Files:**
- Create: `neuron-state-graph/src/schema/node.rs`
- Create: `neuron-state-graph/src/schema/edge.rs`
- Modify: `neuron-state-graph/src/schema/mod.rs`

**Step 1: Write node struct with tests**

```rust
// neuron-state-graph/src/schema/node.rs
//! Graph node types.

use super::{Layer, NodeType};
use serde::{Deserialize, Serialize};

/// A node in the memory graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Unique identifier.
    pub uid: String,
    /// The type of node.
    pub node_type: NodeType,
    /// The layer this node belongs to.
    pub layer: Layer,
    /// Human-readable label.
    pub label: String,
    /// Optional summary text (used for FTS and embeddings).
    pub summary: Option<String>,
    /// Relevance score (decays over time).
    pub salience: f64,
    /// Whether this node has been soft-deleted.
    pub is_tombstoned: bool,
    /// Creation timestamp (unix seconds).
    pub created_at: f64,
    /// Last access timestamp (unix seconds).
    pub last_accessed: f64,
    /// Which agent created/last modified this node.
    pub changed_by: Option<String>,
    /// Arbitrary typed properties.
    pub props: serde_json::Value,
}

/// Parameters for creating a new node.
#[derive(Debug, Clone)]
pub struct CreateNode {
    /// Human-readable label.
    pub label: String,
    /// The type of node.
    pub node_type: NodeType,
    /// Optional summary text.
    pub summary: Option<String>,
    /// Initial salience (default 1.0).
    pub salience: f64,
    /// Agent creating this node.
    pub changed_by: Option<String>,
    /// Arbitrary properties.
    pub props: serde_json::Value,
}

impl CreateNode {
    /// Create a new node with the given label and type.
    pub fn new(label: impl Into<String>, node_type: NodeType) -> Self {
        Self {
            label: label.into(),
            node_type,
            summary: None,
            salience: 1.0,
            changed_by: None,
            props: serde_json::Value::Object(Default::default()),
        }
    }

    /// Set the summary.
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Set the creating agent.
    pub fn changed_by(mut self, agent: impl Into<String>) -> Self {
        self.changed_by = Some(agent.into());
        self
    }

    /// Set properties.
    pub fn props(mut self, props: serde_json::Value) -> Self {
        self.props = props;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_node_builder() {
        let cn = CreateNode::new("Test Entity", NodeType::Entity)
            .summary("A test entity")
            .changed_by("agent-1");

        assert_eq!(cn.label, "Test Entity");
        assert_eq!(cn.node_type, NodeType::Entity);
        assert_eq!(cn.summary.as_deref(), Some("A test entity"));
        assert_eq!(cn.changed_by.as_deref(), Some("agent-1"));
        assert_eq!(cn.salience, 1.0);
    }

    #[test]
    fn graph_node_serialization_roundtrip() {
        let node = GraphNode {
            uid: "n-1".into(),
            node_type: NodeType::Claim,
            layer: Layer::Epistemic,
            label: "Test".into(),
            summary: Some("A test claim".into()),
            salience: 0.9,
            is_tombstoned: false,
            created_at: 1709164800.0,
            last_accessed: 1709164800.0,
            changed_by: Some("agent-1".into()),
            props: serde_json::json!({"confidence": 0.95}),
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: GraphNode = serde_json::from_str(&json).unwrap();
        assert_eq!(back.uid, "n-1");
        assert_eq!(back.node_type, NodeType::Claim);
    }
}
```

**Step 2: Write edge struct** (similar pattern — `GraphEdge` with `from_uid`, `to_uid`, `edge_type`, `weight`, `valid_at`, `invalid_at`, `is_tombstoned`, `created_at`, `props`)

**Step 3: Wire up in mod.rs, run tests, commit**

```bash
git commit -m "feat(state-graph): add GraphNode, GraphEdge, CreateNode structs"
```

---

## Phase 2: CozoDB Engine + Scope Mapping

### Task 5: Implement scope serialization

**Files:**
- Create: `neuron-state-graph/src/scope.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use layer0::effect::Scope;
    use layer0::id::*;

    #[test]
    fn scope_to_string_deterministic() {
        let s1 = scope_to_string(&Scope::Global);
        let s2 = scope_to_string(&Scope::Global);
        assert_eq!(s1, s2);
        assert_eq!(s1, "global");
    }

    #[test]
    fn different_scopes_different_strings() {
        let global = scope_to_string(&Scope::Global);
        let session = scope_to_string(&Scope::Session(SessionId::new("s1")));
        let workflow = scope_to_string(&Scope::Workflow(WorkflowId::new("wf1")));
        let agent = scope_to_string(&Scope::Agent {
            workflow: WorkflowId::new("wf1"),
            agent: AgentId::new("a1"),
        });
        let custom = scope_to_string(&Scope::Custom("research/topic".into()));

        assert_ne!(global, session);
        assert_ne!(session, workflow);
        assert_ne!(workflow, agent);
        assert_eq!(global, "global");
        assert_eq!(session, "session:s1");
        assert_eq!(workflow, "workflow:wf1");
        assert_eq!(agent, "agent:wf1:a1");
        assert_eq!(custom, "custom:research/topic");
    }
}
```

**Step 2: Implement**

```rust
// neuron-state-graph/src/scope.rs
//! Scope-to-string mapping for CozoDB composite keys.

use layer0::effect::Scope;

/// Convert a neuron Scope to a deterministic string for use as a CozoDB key component.
pub fn scope_to_string(scope: &Scope) -> String {
    match scope {
        Scope::Global => "global".into(),
        Scope::Session(id) => format!("session:{}", id.as_str()),
        Scope::Workflow(id) => format!("workflow:{}", id.as_str()),
        Scope::Agent { workflow, agent } => {
            format!("agent:{}:{}", workflow.as_str(), agent.as_str())
        }
        Scope::Custom(s) => format!("custom:{s}"),
    }
}
```

**Step 3: Run tests, commit**

```bash
git commit -m "feat(state-graph): add scope-to-string mapping"
```

---

### Task 6: CozoDB engine wrapper + schema migration

**Files:**
- Create: `neuron-state-graph/src/engine.rs`
- Create: `neuron-state-graph/src/migration.rs`

**Step 1: Write failing test for engine creation**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_in_memory_engine() {
        let engine = CozoEngine::open_in_memory().unwrap();
        assert!(engine.is_initialized());
    }
}
```

**Step 2: Implement CozoEngine**

```rust
// neuron-state-graph/src/engine.rs
//! Typed wrapper around CozoDB's DbInstance.

use cozo::DbInstance;
use layer0::error::StateError;
use std::path::Path;

/// Typed wrapper around CozoDB providing safe query execution.
pub struct CozoEngine {
    db: DbInstance,
}

impl CozoEngine {
    /// Open a persistent CozoDB backed by SQLite at the given path.
    pub fn open(path: &Path) -> Result<Self, StateError> {
        let db = DbInstance::new("sqlite", path.to_str().unwrap_or(""), "")
            .map_err(|e| StateError::WriteFailed(format!("failed to open CozoDB: {e}")))?;
        let engine = Self { db };
        engine.run_migrations()?;
        Ok(engine)
    }

    /// Open an in-memory CozoDB (for testing).
    pub fn open_in_memory() -> Result<Self, StateError> {
        let db = DbInstance::new("mem", "", "")
            .map_err(|e| StateError::WriteFailed(format!("failed to open CozoDB: {e}")))?;
        let engine = Self { db };
        engine.run_migrations()?;
        Ok(engine)
    }

    /// Check if the engine is initialized (migrations have run).
    pub fn is_initialized(&self) -> bool {
        self.run_query("?[x] := *node{uid: x}", Default::default())
            .is_ok()
    }

    /// Execute a CozoScript query with parameters.
    pub fn run_query(
        &self,
        script: &str,
        params: std::collections::BTreeMap<String, cozo::DataValue>,
    ) -> Result<cozo::NamedRows, StateError> {
        self.db
            .run_script(script, params, cozo::ScriptMutability::Mutable)
            .map_err(|e| StateError::WriteFailed(format!("CozoDB query error: {e}")))
    }

    /// Execute a read-only query.
    pub fn run_query_readonly(
        &self,
        script: &str,
        params: std::collections::BTreeMap<String, cozo::DataValue>,
    ) -> Result<cozo::NamedRows, StateError> {
        self.db
            .run_script(script, params, cozo::ScriptMutability::Immutable)
            .map_err(|e| StateError::WriteFailed(format!("CozoDB query error: {e}")))
    }
}
```

**Step 3: Implement migrations**

```rust
// neuron-state-graph/src/migration.rs
//! CozoDB schema DDL — creates relations and indices.

use crate::engine::CozoEngine;
use layer0::error::StateError;

impl CozoEngine {
    /// Run schema migrations to create all required relations and indices.
    pub(crate) fn run_migrations(&self) -> Result<(), StateError> {
        let schema = r#"
            {
                :create node {
                    uid: String
                    =>
                    scope: String,
                    node_type: String,
                    layer: String,
                    label: String,
                    summary: String default "",
                    salience: Float default 1.0,
                    is_tombstoned: Bool default false,
                    created_at: Float,
                    last_accessed: Float,
                    changed_by: String default "",
                    props: Json default {}
                }
            }
            {
                :create edge {
                    uid: String
                    =>
                    scope: String,
                    from_uid: String,
                    to_uid: String,
                    edge_type: String,
                    weight: Float default 1.0,
                    valid_at: Float default 0.0,
                    invalid_at: Float default 0.0,
                    is_tombstoned: Bool default false,
                    created_at: Float,
                    changed_by: String default "",
                    props: Json default {}
                }
            }
            {
                :create kv {
                    scope: String,
                    key: String
                    =>
                    value: Json
                }
            }
        "#;

        // Run each create statement separately — CozoDB doesn't
        // support multiple :create in one script.
        for stmt in schema.split("\n            {") {
            let stmt = stmt.trim();
            if stmt.is_empty() {
                continue;
            }
            let stmt = if stmt.starts_with('{') {
                stmt.to_string()
            } else {
                format!("{{{stmt}")
            };
            // Ignore "already exists" errors on re-run.
            let _ = self.run_query(&stmt, Default::default());
        }

        // Create indices (ignore if exist).
        let indices = [
            "::index create node:scope_idx {scope}",
            "::index create node:type_idx {node_type}",
            "::index create node:layer_idx {layer}",
            "::index create edge:from_idx {from_uid}",
            "::index create edge:to_idx {to_uid}",
            "::index create edge:type_idx {edge_type}",
            "::index create edge:scope_idx {scope}",
        ];
        for idx in &indices {
            let _ = self.run_query(idx, Default::default());
        }

        // Create FTS indices (ignore if exist).
        let fts = [
            "::fts create node:label_fts {extractor: label, tokenizer: Simple, filters: [Lowercase]}",
            "::fts create node:summary_fts {extractor: summary, tokenizer: Simple, filters: [Lowercase]}",
        ];
        for f in &fts {
            let _ = self.run_query(f, Default::default());
        }

        Ok(())
    }
}
```

**Step 4: Run tests, commit**

```bash
git commit -m "feat(state-graph): add CozoEngine wrapper and schema migrations"
```

---

## Phase 3: StateStore CRUD Implementation

### Task 7: Implement StateStore read/write/delete/list

**Files:**
- Create: `neuron-state-graph/src/store.rs`
- Modify: `neuron-state-graph/src/lib.rs`

This task implements the core `StateStore` trait using the `kv` relation for simple key-value operations (matching existing backends). Graph-aware operations come later.

**Step 1: Write failing tests** (mirror the tests from `neuron-state-memory` and `neuron-state-fs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use layer0::effect::Scope;
    use layer0::state::StateStore;
    use serde_json::json;

    fn test_store() -> GraphStore {
        GraphStore::open_in_memory().unwrap()
    }

    #[tokio::test]
    async fn write_and_read() {
        let store = test_store();
        let scope = Scope::Global;
        store.write(&scope, "key1", json!("value1")).await.unwrap();
        let val = store.read(&scope, "key1").await.unwrap();
        assert_eq!(val, Some(json!("value1")));
    }

    #[tokio::test]
    async fn read_nonexistent_returns_none() {
        let store = test_store();
        let val = store.read(&Scope::Global, "missing").await.unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn write_overwrites() {
        let store = test_store();
        let scope = Scope::Global;
        store.write(&scope, "k", json!("first")).await.unwrap();
        store.write(&scope, "k", json!("second")).await.unwrap();
        assert_eq!(store.read(&scope, "k").await.unwrap(), Some(json!("second")));
    }

    #[tokio::test]
    async fn delete_removes() {
        let store = test_store();
        let scope = Scope::Global;
        store.write(&scope, "k", json!("v")).await.unwrap();
        store.delete(&scope, "k").await.unwrap();
        assert_eq!(store.read(&scope, "k").await.unwrap(), None);
    }

    #[tokio::test]
    async fn delete_nonexistent_is_ok() {
        let store = test_store();
        assert!(store.delete(&Scope::Global, "missing").await.is_ok());
    }

    #[tokio::test]
    async fn list_with_prefix() {
        let store = test_store();
        let scope = Scope::Global;
        store.write(&scope, "user:name", json!("Alice")).await.unwrap();
        store.write(&scope, "user:age", json!(30)).await.unwrap();
        store.write(&scope, "system:v", json!("1.0")).await.unwrap();
        let mut keys = store.list(&scope, "user:").await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["user:age", "user:name"]);
    }

    #[tokio::test]
    async fn scopes_are_isolated() {
        let store = test_store();
        let g = Scope::Global;
        let s = Scope::Session(layer0::SessionId::new("s1"));
        store.write(&g, "key", json!("global")).await.unwrap();
        store.write(&s, "key", json!("session")).await.unwrap();
        assert_eq!(store.read(&g, "key").await.unwrap(), Some(json!("global")));
        assert_eq!(store.read(&s, "key").await.unwrap(), Some(json!("session")));
    }

    #[tokio::test]
    async fn search_returns_empty_without_embeddings() {
        let store = test_store();
        let results = store.search(&Scope::Global, "query", 10).await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn graph_store_implements_state_store() {
        fn _assert<T: StateStore>() {}
        _assert::<GraphStore>();
    }
}
```

**Step 2: Run tests — all should fail**

Run: `nix develop -c cargo test -p neuron-state-graph`
Expected: FAIL — GraphStore not defined

**Step 3: Implement GraphStore**

```rust
// neuron-state-graph/src/store.rs
//! GraphStore — StateStore implementation backed by CozoDB.

use crate::engine::CozoEngine;
use crate::scope::scope_to_string;
use async_trait::async_trait;
use cozo::DataValue;
use layer0::effect::Scope;
use layer0::error::StateError;
use layer0::state::{SearchResult, StateStore};
use std::collections::BTreeMap;

/// A `StateStore` backed by CozoDB with graph-structured knowledge.
pub struct GraphStore {
    engine: CozoEngine,
}

impl GraphStore {
    /// Open a persistent graph store at the given path.
    pub fn open(path: &std::path::Path) -> Result<Self, StateError> {
        Ok(Self {
            engine: CozoEngine::open(path)?,
        })
    }

    /// Open an in-memory graph store (for testing).
    pub fn open_in_memory() -> Result<Self, StateError> {
        Ok(Self {
            engine: CozoEngine::open_in_memory()?,
        })
    }

    fn params(&self, scope: &Scope, key: &str) -> BTreeMap<String, DataValue> {
        let mut p = BTreeMap::new();
        p.insert("scope".into(), DataValue::Str(scope_to_string(scope).into()));
        p.insert("key".into(), DataValue::Str(key.into()));
        p
    }
}

#[async_trait]
impl StateStore for GraphStore {
    async fn read(&self, scope: &Scope, key: &str)
    -> Result<Option<serde_json::Value>, StateError> {
        let params = self.params(scope, key);
        let result = self.engine.run_query_readonly(
            "?[value] := *kv{scope: $scope, key: $key, value}",
            params,
        )?;
        if result.rows.is_empty() {
            Ok(None)
        } else {
            let val = &result.rows[0][0];
            let json = datavalue_to_json(val);
            Ok(Some(json))
        }
    }

    async fn write(&self, scope: &Scope, key: &str, value: serde_json::Value)
    -> Result<(), StateError> {
        let mut params = self.params(scope, key);
        params.insert("value".into(), json_to_datavalue(&value));
        self.engine.run_query(
            "?[scope, key, value] <- [[$scope, $key, $value]] :put kv {scope, key => value}",
            params,
        )?;
        Ok(())
    }

    async fn delete(&self, scope: &Scope, key: &str) -> Result<(), StateError> {
        let params = self.params(scope, key);
        let _ = self.engine.run_query(
            "?[scope, key] <- [[$scope, $key]] :rm kv {scope, key}",
            params,
        );
        Ok(())
    }

    async fn list(&self, scope: &Scope, prefix: &str) -> Result<Vec<String>, StateError> {
        let mut params = BTreeMap::new();
        params.insert("scope".into(), DataValue::Str(scope_to_string(scope).into()));
        params.insert("prefix".into(), DataValue::Str(prefix.into()));
        let result = self.engine.run_query_readonly(
            "?[key] := *kv{scope: $scope, key}, starts_with(key, $prefix)",
            params,
        )?;
        let keys = result.rows.iter().filter_map(|row| {
            match &row[0] {
                DataValue::Str(s) => Some(s.to_string()),
                _ => None,
            }
        }).collect();
        Ok(keys)
    }

    async fn search(&self, _scope: &Scope, _query: &str, _limit: usize)
    -> Result<Vec<SearchResult>, StateError> {
        // Phase 4 adds real search. For now, match existing backends.
        Ok(vec![])
    }
}

/// Convert CozoDB DataValue to serde_json::Value.
fn datavalue_to_json(dv: &DataValue) -> serde_json::Value {
    match dv {
        DataValue::Null => serde_json::Value::Null,
        DataValue::Bool(b) => serde_json::Value::Bool(*b),
        DataValue::Num(n) => {
            if let Some(i) = n.get_int() {
                serde_json::Value::Number(i.into())
            } else if let Some(f) = n.get_float() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        DataValue::Str(s) => serde_json::Value::String(s.to_string()),
        DataValue::List(l) => {
            serde_json::Value::Array(l.iter().map(datavalue_to_json).collect())
        }
        DataValue::Json(j) => j.0.clone(),
        _ => serde_json::Value::String(format!("{dv:?}")),
    }
}

/// Convert serde_json::Value to CozoDB DataValue.
fn json_to_datavalue(v: &serde_json::Value) -> DataValue {
    DataValue::Json(cozo::JsonData(v.clone()))
}
```

**Step 4: Wire up in lib.rs**

```rust
pub mod engine;
pub mod migration;
pub mod schema;
pub mod scope;
pub mod store;

pub use store::GraphStore;
```

**Step 5: Run tests**

Run: `nix develop -c cargo test -p neuron-state-graph`
Expected: All tests pass

**Step 6: Run clippy**

Run: `nix develop -c cargo clippy -p neuron-state-graph -- -D warnings`
Expected: No warnings

**Step 7: Commit**

```bash
git add neuron-state-graph/src/
git commit -m "feat(state-graph): implement StateStore CRUD on CozoDB"
```

---

## Phase 4: Graph Operations

### Task 8: Node and edge CRUD on CozoDB

**Files:**
- Modify: `neuron-state-graph/src/store.rs`

Add methods for creating/reading/tombstoning nodes and edges in the graph relations. These are internal methods used by the retrieval pipeline and the future `GraphStateStore` trait.

**Step 1: Write tests for add_node, get_node, add_edge**

**Step 2: Implement using CozoDB `:put` queries with UUID generation**

**Step 3: Run tests, commit**

```bash
git commit -m "feat(state-graph): add graph node and edge CRUD"
```

---

### Task 9: Graph traversal (2-query BFS)

**Files:**
- Create: `neuron-state-graph/src/traversal.rs`

Implement the 2-query BFS pattern (from MindGraph analysis): fetch all live edges in scope → BFS in-memory → batch-fetch node metadata.

**Step 1: Write tests for traverse_from(scope, uid, depth, direction)**

**Step 2: Implement BFS with configurable depth and direction**

**Step 3: Run tests, commit**

```bash
git commit -m "feat(state-graph): add 2-query BFS graph traversal"
```

---

## Phase 5: Retrieval Pipeline

### Task 10: FTS search via CozoDB

**Files:**
- Modify: `neuron-state-graph/src/store.rs` (or create `retrieval/` module)

**Step 1: Write test — store nodes with labels, search by text, expect ranked results**

**Step 2: Implement FTS query using CozoDB `~node:label_fts{...}` and `~node:summary_fts{...}`**

**Step 3: Return results as `Vec<SearchResult>` from `StateStore::search()`**

**Step 4: Run tests, commit**

```bash
git commit -m "feat(state-graph): add FTS search via CozoDB indices"
```

---

### Task 11: Embedding provider trait + HNSW setup

**Files:**
- Create: `neuron-state-graph/src/embedding.rs`

**Step 1: Define EmbeddingProvider trait**

```rust
/// A provider that generates embedding vectors from text.
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding for the given text.
    fn embed(&self, text: &str) -> Result<Vec<f32>, StateError>;

    /// The dimensionality of embeddings this provider produces.
    fn dimension(&self) -> usize;
}
```

**Step 2: Implement MockEmbeddingProvider for testing (returns deterministic vectors)**

**Step 3: Add HNSW index creation to migrations when embedding dimension is configured**

**Step 4: Implement `embed_node()` and `semantic_search()` on GraphStore**

**Step 5: Run tests, commit**

```bash
git commit -m "feat(state-graph): add embedding provider trait and HNSW search"
```

---

### Task 12: Hybrid retrieval pipeline (RRF + MMR)

**Files:**
- Create: `neuron-state-graph/src/retrieval/mod.rs`
- Create: `neuron-state-graph/src/retrieval/local.rs`
- Create: `neuron-state-graph/src/retrieval/hybrid.rs`

**Step 1: Implement RRF fusion**

```rust
/// Reciprocal Rank Fusion: combines ranked lists without score normalization.
/// RRF(d) = sum(1 / (k + rank_i(d))) for each ranking list i
pub fn rrf_fusion(ranked_lists: &[Vec<(String, f64)>], k: f64) -> Vec<(String, f64)> {
    // ...
}
```

**Step 2: Implement MMR diversity filter**

**Step 3: Implement LocalSearch pipeline: HNSW top-20 + BM25 top-20 → graph expand → RRF → MMR**

**Step 4: Wire into `StateStore::search()` — when embeddings are configured, use hybrid pipeline; otherwise fall back to FTS-only**

**Step 5: Run tests, commit**

```bash
git commit -m "feat(state-graph): add hybrid retrieval pipeline (RRF + MMR)"
```

---

## Phase 6: Extension Trait + Effects

### Task 13: GraphStateStore extension trait

**Files:**
- Create: `neuron-state-graph/src/graph_trait.rs`

**Step 1: Define trait**

```rust
/// Extension trait for graph-aware state operations.
/// Implemented by backends that support knowledge graph structure.
#[async_trait]
pub trait GraphStateStore: StateStore {
    /// Create a relationship between two keys.
    async fn link(
        &self, scope: &Scope, from_key: &str, relation: &str,
        to_key: &str, metadata: Option<serde_json::Value>,
    ) -> Result<(), StateError>;

    /// Remove a relationship.
    async fn unlink(
        &self, scope: &Scope, from_key: &str, relation: &str, to_key: &str,
    ) -> Result<(), StateError>;

    /// Find related keys via graph traversal.
    async fn traverse(
        &self, scope: &Scope, from_key: &str, relation: Option<&str>,
        depth: usize, limit: usize,
    ) -> Result<Vec<TraversalResult>, StateError>;
}
```

**Step 2: Implement for GraphStore**

**Step 3: Run tests, commit**

```bash
git commit -m "feat(state-graph): add GraphStateStore extension trait"
```

---

### Task 14: Add LinkMemory/UnlinkMemory effect variants

**Files:**
- Modify: `layer0/src/effect.rs`

**Step 1: Add new variants to Effect enum**

```rust
/// Create a relationship between two memory keys.
LinkMemory {
    scope: Scope,
    from_key: String,
    relation: String,
    to_key: String,
    metadata: Option<serde_json::Value>,
},

/// Remove a relationship between two memory keys.
UnlinkMemory {
    scope: Scope,
    from_key: String,
    relation: String,
    to_key: String,
},
```

**Step 2: Update serialization tests**

**Step 3: Run full workspace tests**

Run: `nix develop -c cargo test --workspace --all-targets`
Expected: All pass

**Step 4: Commit**

```bash
git commit -m "feat(layer0): add LinkMemory and UnlinkMemory effect variants"
```

---

## Phase 7: Compaction

### Task 15: Salience decay + auto-tombstone

**Files:**
- Create: `neuron-state-graph/src/compaction.rs`

**Step 1: Implement decay_salience function**

**Step 2: Implement auto_tombstone (tombstone nodes below threshold)**

**Step 3: Write tests with time-based decay assertions**

**Step 4: Run tests, commit**

```bash
git commit -m "feat(state-graph): add salience decay and auto-tombstone"
```

---

## Phase 8: Integration + Polish

### Task 16: Integration tests with full pipeline

**Files:**
- Create: `neuron-state-graph/tests/integration.rs`

Write end-to-end tests:
1. Create graph store
2. Write nodes with embeddings
3. Create edges between nodes
4. Search and verify GraphRAG results (seed → expand → rank)
5. Verify scope isolation
6. Verify tombstoning excludes from results
7. Verify salience decay affects ranking

**Step 1: Write integration tests**

**Step 2: Run full test suite**

Run: `nix develop -c cargo test --workspace --all-targets`
Expected: All pass

**Step 3: Run clippy on full workspace**

Run: `nix develop -c cargo clippy --workspace --all-targets -- -D warnings`
Expected: No warnings

**Step 4: Commit**

```bash
git commit -m "test(state-graph): add integration tests for full GraphRAG pipeline"
```

---

## Task Dependencies

```
Task 1 (crate skeleton)
  └─► Task 2 (Layer, NodeType)
  └─► Task 3 (EdgeType)
       └─► Task 4 (GraphNode, GraphEdge)
  └─► Task 5 (scope mapping)
       └─► Task 6 (CozoDB engine + migrations)
            └─► Task 7 (StateStore CRUD) ◄── KEY MILESTONE: trait compliance
                 └─► Task 8 (graph node/edge CRUD)
                      └─► Task 9 (BFS traversal)
                 └─► Task 10 (FTS search)
                 └─► Task 11 (embeddings + HNSW)
                      └─► Task 12 (hybrid retrieval) ◄── KEY MILESTONE: GraphRAG works
                 └─► Task 13 (GraphStateStore trait)
                 └─► Task 14 (Effect variants)
                 └─► Task 15 (compaction)
                      └─► Task 16 (integration tests) ◄── KEY MILESTONE: done
```

Tasks 2, 3, 5 can run in parallel after Task 1.
Tasks 10, 11, 13, 14, 15 can run in parallel after Task 7.
