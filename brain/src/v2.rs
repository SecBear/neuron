//! Brain v2: structured research backend (async jobs + grounded bundles).

use base64::Engine;
use chrono::{DateTime, Utc};
use neuron_tool::{ToolDyn, ToolError, ToolRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Status for an async research job.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Job created but not started yet.
    Pending,
    /// Job currently executing.
    Running,
    /// Job completed successfully.
    Succeeded,
    /// Job failed.
    Failed,
    /// Job was canceled.
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobInputs {
    intent: String,
    #[serde(default)]
    constraints: Value,
    #[serde(default)]
    targets: Vec<Value>,
    #[serde(default)]
    tool_policy: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundlePointer {
    artifact_root: String,
    index_path: String,
    findings_path: String,
}

#[derive(Debug)]
struct JobEntry {
    created_at: DateTime<Utc>,
    status: JobStatus,
    inputs: JobInputs,
    error: Option<String>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobRecord {
    job_id: String,
    created_at: String,
    status: JobStatus,
    inputs: JobInputs,
    error: Option<String>,
}

/// Shared manager for research jobs.
///
/// The MCP-exposed tools call into this manager to start/poll/get/cancel jobs and inspect artifacts.
pub struct JobManager {
    artifact_root: PathBuf,
    acquisition: Arc<ToolRegistry>,
    jobs: Mutex<HashMap<String, JobEntry>>,
}

impl JobManager {
    /// Create a new job manager.
    pub fn new(artifact_root: PathBuf, acquisition: ToolRegistry) -> Self {
        std::fs::create_dir_all(&artifact_root).ok();

        let mut initial = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&artifact_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let job_id = match path.file_name().and_then(|s| s.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                };
                let job_path = path.join("job.json");
                if !job_path.exists() {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&job_path) {
                    if let Ok(record) = serde_json::from_str::<JobRecord>(&content) {
                        if let Ok(created_at) = parse_rfc3339_utc(&record.created_at) {
                            let mut status = record.status;
                            let mut error = record.error;
                            // Pragmatic restart semantics: running/pending can't be resumed yet.
                            if matches!(status, JobStatus::Running | JobStatus::Pending) {
                                status = JobStatus::Failed;
                                if error.is_none() {
                                    error = Some(
                                        "job was running during process restart; not resumable"
                                            .to_string(),
                                    );
                                }
                                let updated = JobRecord {
                                    job_id: job_id.clone(),
                                    created_at: record.created_at,
                                    status: status.clone(),
                                    inputs: record.inputs.clone(),
                                    error: error.clone(),
                                };
                                let _ = std::fs::write(
                                    &job_path,
                                    serde_json::to_string_pretty(&updated)
                                        .unwrap_or_else(|_| "{}".to_string()),
                                );
                            }

                            initial.insert(
                                job_id,
                                JobEntry {
                                    created_at,
                                    status,
                                    inputs: record.inputs,
                                    error,
                                    handle: None,
                                },
                            );
                        }
                    }
                }
            }
        }

        Self {
            artifact_root,
            acquisition: Arc::new(acquisition),
            jobs: Mutex::new(initial),
        }
    }

    fn job_dir(&self, job_id: &str) -> PathBuf {
        self.artifact_root.join(job_id)
    }

    fn job_record_path(&self, job_id: &str) -> PathBuf {
        self.job_dir(job_id).join("job.json")
    }

    fn write_job_record(&self, job_id: &str, entry: &JobEntry) -> Result<(), ToolError> {
        let record = JobRecord {
            job_id: job_id.to_string(),
            created_at: entry.created_at.to_rfc3339(),
            status: entry.status.clone(),
            inputs: entry.inputs.clone(),
            error: entry.error.clone(),
        };
        let path = self.job_record_path(job_id);
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&record)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
        )
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(())
    }

    fn try_load_job_record(&self, job_id: &str) -> Option<JobEntry> {
        let path = self.job_record_path(job_id);
        let content = std::fs::read_to_string(&path).ok()?;
        let record = serde_json::from_str::<JobRecord>(&content).ok()?;
        let created_at = parse_rfc3339_utc(&record.created_at).ok()?;
        Some(JobEntry {
            created_at,
            status: record.status,
            inputs: record.inputs,
            error: record.error,
            handle: None,
        })
    }

    async fn start_job(self: &Arc<Self>, inputs: JobInputs) -> Result<String, ToolError> {
        let job_id = Uuid::new_v4().to_string();
        let artifact_dir = self.job_dir(&job_id);
        std::fs::create_dir_all(&artifact_dir)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        let created_at = Utc::now();

        let mut jobs = self.jobs.lock().await;
        jobs.insert(
            job_id.clone(),
            JobEntry {
                created_at,
                status: JobStatus::Running,
                inputs: inputs.clone(),
                error: None,
                handle: None,
            },
        );
        if let Some(entry) = jobs.get(&job_id) {
            self.write_job_record(&job_id, entry)?;
        }

        let mgr = Arc::clone(self);
        let job_id_clone = job_id.clone();
        let handle = tokio::spawn(async move {
            let result = run_job(
                &job_id_clone,
                created_at,
                &inputs,
                &mgr.artifact_root,
                Arc::clone(&mgr.acquisition),
            )
            .await;

            let mut jobs = mgr.jobs.lock().await;
            if let Some(entry) = jobs.get_mut(&job_id_clone) {
                match result {
                    Ok(()) => entry.status = JobStatus::Succeeded,
                    Err(err) => {
                        if entry.status != JobStatus::Canceled {
                            entry.status = JobStatus::Failed;
                            entry.error = Some(err);
                        }
                    }
                }
                entry.handle = None;
                let _ = mgr.write_job_record(&job_id_clone, entry);
            }
        });

        if let Some(entry) = jobs.get_mut(&job_id) {
            entry.handle = Some(handle);
            self.write_job_record(&job_id, entry)?;
        }

        Ok(job_id)
    }

    async fn status(&self, job_id: &str) -> Result<Value, ToolError> {
        let mut jobs = self.jobs.lock().await;
        if !jobs.contains_key(job_id) {
            if let Some(loaded) = self.try_load_job_record(job_id) {
                jobs.insert(job_id.to_string(), loaded);
            }
        }
        let entry = jobs
            .get(job_id)
            .ok_or_else(|| ToolError::NotFound(format!("job not found: {job_id}")))?;
        Ok(serde_json::json!({
            "job_id": job_id,
            "status": entry.status,
            "created_at": entry.created_at.to_rfc3339(),
            "inputs": serde_json::to_value(&entry.inputs).unwrap_or(Value::Null),
            "progress": {
                "artifact_root": self.artifact_root.to_string_lossy()
            }
        }))
    }

    async fn get_bundle(&self, job_id: &str) -> Result<Value, ToolError> {
        let mut jobs = self.jobs.lock().await;
        if !jobs.contains_key(job_id) {
            if let Some(loaded) = self.try_load_job_record(job_id) {
                jobs.insert(job_id.to_string(), loaded);
            }
        }
        let entry = jobs
            .get(job_id)
            .ok_or_else(|| ToolError::NotFound(format!("job not found: {job_id}")))?;
        if entry.status != JobStatus::Succeeded {
            return Err(ToolError::ExecutionFailed(format!(
                "job not succeeded: {job_id} status={:?}",
                entry.status
            )));
        }

        let ptr = BundlePointer {
            artifact_root: self.artifact_root.to_string_lossy().to_string(),
            index_path: "index.json".to_string(),
            findings_path: "findings.md".to_string(),
        };

        Ok(serde_json::json!({
            "job_id": job_id,
            "status": JobStatus::Succeeded,
            "bundle": ptr
        }))
    }

    async fn cancel(&self, job_id: &str) -> Result<Value, ToolError> {
        let mut jobs = self.jobs.lock().await;
        let entry = jobs
            .get_mut(job_id)
            .ok_or_else(|| ToolError::NotFound(format!("job not found: {job_id}")))?;

        entry.status = JobStatus::Canceled;
        if let Some(handle) = entry.handle.take() {
            handle.abort();
        }
        self.write_job_record(job_id, entry)?;
        Ok(serde_json::json!({"job_id": job_id, "status": JobStatus::Canceled}))
    }

    async fn artifact_list(&self, job_id: &str, prefix: &str) -> Result<Value, ToolError> {
        let prefix = prefix.to_string();
        let index = self.read_index(job_id).await?;
        let artifacts = index
            .get("artifacts")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let filtered: Vec<Value> = artifacts
            .into_iter()
            .filter_map(|a| {
                let path = a.get("path")?.as_str()?;
                if path.starts_with(&prefix) {
                    Some(serde_json::json!({
                        "path": path,
                        "sha256": a.get("sha256").cloned().unwrap_or(Value::Null)
                    }))
                } else {
                    None
                }
            })
            .collect();

        Ok(serde_json::json!({ "artifacts": filtered }))
    }

    async fn artifact_read(&self, job_id: &str, path: &str) -> Result<Value, ToolError> {
        let rel =
            validate_relative_path(path).map_err(|e| ToolError::InvalidInput(e.to_string()))?;
        let full = self.job_dir(job_id).join(&rel);
        let bytes = std::fs::read(&full).map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let sha256 = sha256_hex(&bytes);
        match String::from_utf8(bytes) {
            Ok(content) => Ok(serde_json::json!({
                "path": rel.to_string_lossy(),
                "sha256": sha256,
                "encoding": "utf-8",
                "content": content
            })),
            Err(e) => {
                let raw = e.into_bytes();
                let b64 = base64::engine::general_purpose::STANDARD.encode(raw);
                Ok(serde_json::json!({
                    "path": rel.to_string_lossy(),
                    "sha256": sha256,
                    "encoding": "base64",
                    "content_base64": b64
                }))
            }
        }
    }

    async fn read_index(&self, job_id: &str) -> Result<Value, ToolError> {
        let path = self.job_dir(job_id).join("index.json");
        let content = std::fs::read_to_string(&path)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        serde_json::from_str(&content).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
    }
}

/// Build a ToolRegistry exposing Brain v2 MCP tool surface.
pub fn backend_registry(manager: Arc<JobManager>) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ResearchJobStartTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(ResearchJobStatusTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(ResearchJobGetTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(ResearchJobCancelTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(ArtifactListTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(ArtifactReadTool::new(Arc::clone(&manager))));
    registry
}

async fn run_job(
    job_id: &str,
    created_at: DateTime<Utc>,
    inputs: &JobInputs,
    artifact_root: &Path,
    acquisition: Arc<ToolRegistry>,
) -> Result<(), String> {
    let job_dir = artifact_root.join(job_id);
    std::fs::create_dir_all(job_dir.join("sources")).map_err(|e| e.to_string())?;

    // Minimal acquisition: if a tool named "web_search" exists, call it with {"query": intent}.
    let mut artifacts = Vec::<Value>::new();
    let retrieved_at = Utc::now().to_rfc3339();
    let primary_source_path: String;
    if let Some(tool) = acquisition.get("web_search") {
        let out = tool
            .call(serde_json::json!({ "query": inputs.intent }))
            .await
            .map_err(|e| e.to_string())?;
        let pretty = serde_json::to_string_pretty(&out).unwrap_or_else(|_| out.to_string());
        let rel_path = PathBuf::from("sources/web_search.json");
        let full_path = job_dir.join(&rel_path);
        std::fs::write(&full_path, pretty.as_bytes()).map_err(|e| e.to_string())?;
        let sha = sha256_hex(pretty.as_bytes());
        primary_source_path = rel_path.to_string_lossy().to_string();
        artifacts.push(serde_json::json!({
            "path": rel_path.to_string_lossy(),
            "sha256": sha,
            "media_type": "application/json",
            "retrieved_at": retrieved_at,
            "source_url": Value::Null
        }));
    } else {
        let rel_path = PathBuf::from("sources/intent.txt");
        let full_path = job_dir.join(&rel_path);
        std::fs::write(&full_path, inputs.intent.as_bytes()).map_err(|e| e.to_string())?;
        let sha = sha256_hex(inputs.intent.as_bytes());
        primary_source_path = rel_path.to_string_lossy().to_string();
        artifacts.push(serde_json::json!({
            "path": rel_path.to_string_lossy(),
            "sha256": sha,
            "media_type": "text/plain",
            "retrieved_at": retrieved_at,
            "source_url": Value::Null
        }));
    }

    let index = serde_json::json!({
        "job": {
            "id": job_id,
            "created_at": created_at.to_rfc3339(),
            "status": "succeeded",
            "inputs": {
                "intent": inputs.intent,
                "constraints": inputs.constraints,
                "targets": inputs.targets,
                "tool_policy": inputs.tool_policy
            }
        },
        "artifacts": artifacts,
        "claims": [
            {
                "id": "claim_1",
                "kind": "fact",
                "statement": "Acquisition produced an initial source snapshot.",
                "evidence": [
                    {
                        "artifact_path": primary_source_path,
                        "excerpt": "See sources for details.",
                        "locator": Value::Null,
                        "retrieved_at": retrieved_at,
                        "source_url": Value::Null
                    }
                ]
            }
        ],
        "coverage": {
            "targets": [],
            "gaps": []
        },
        "next_steps": []
    });

    // Groundedness gate: ensure all fact claims have evidence.
    enforce_groundedness(&index)?;

    std::fs::write(
        job_dir.join("index.json"),
        serde_json::to_string_pretty(&index).unwrap(),
    )
    .map_err(|e| e.to_string())?;

    let findings = format!(
        "# Findings\n\nJob `{}` completed.\n\n- Intent: `{}`\n- Artifacts: {}\n",
        job_id,
        index["job"]["inputs"]["intent"]
            .as_str()
            .unwrap_or_default(),
        index["artifacts"].as_array().map(|a| a.len()).unwrap_or(0)
    );
    std::fs::write(job_dir.join("findings.md"), findings).map_err(|e| e.to_string())?;

    Ok(())
}

fn enforce_groundedness(index: &Value) -> Result<(), String> {
    let claims = index
        .get("claims")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing claims".to_string())?;
    for claim in claims {
        let kind = claim.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        if kind == "fact" {
            let evidence = claim.get("evidence").and_then(|v| v.as_array());
            if evidence.is_none() || evidence.is_some_and(|e| e.is_empty()) {
                return Err("fact claim missing evidence".to_string());
            }
        }
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return Err("path must not be absolute".to_string());
    }
    for component in candidate.components() {
        match component {
            Component::ParentDir => return Err("path must not contain '..'".to_string()),
            Component::Prefix(_) => return Err("path must be relative".to_string()),
            _ => {}
        }
    }
    Ok(candidate)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn parse_rfc3339_utc(text: &str) -> Result<DateTime<Utc>, String> {
    let parsed = DateTime::parse_from_rfc3339(text).map_err(|e| e.to_string())?;
    Ok(parsed.with_timezone(&Utc))
}

struct ResearchJobStartTool {
    mgr: Arc<JobManager>,
}

impl ResearchJobStartTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for ResearchJobStartTool {
    fn name(&self) -> &str {
        "research_job_start"
    }
    fn description(&self) -> &str {
        "Start an async research job and return a job id."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "intent":{"type":"string"},
                "constraints":{"type":"object"},
                "targets":{"type":"array"},
                "tool_policy":{"type":"object"}
            },
            "required":["intent"]
        })
    }
    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let intent = input
                .get("intent")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: intent".to_string()))?
                .to_string();

            let inputs = JobInputs {
                intent,
                constraints: input.get("constraints").cloned().unwrap_or(Value::Null),
                targets: input
                    .get("targets")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default(),
                tool_policy: input.get("tool_policy").cloned().unwrap_or(Value::Null),
            };

            let job_id = self.mgr.start_job(inputs).await?;
            Ok(serde_json::json!({ "job_id": job_id, "status": JobStatus::Running }))
        })
    }
}

struct ResearchJobStatusTool {
    mgr: Arc<JobManager>,
}

impl ResearchJobStatusTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for ResearchJobStatusTool {
    fn name(&self) -> &str {
        "research_job_status"
    }
    fn description(&self) -> &str {
        "Get status for a research job."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object",
            "properties":{"job_id":{"type":"string"}},
            "required":["job_id"]
        })
    }
    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let job_id = input
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: job_id".to_string()))?;
            self.mgr.status(job_id).await
        })
    }
}

struct ResearchJobGetTool {
    mgr: Arc<JobManager>,
}

impl ResearchJobGetTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for ResearchJobGetTool {
    fn name(&self) -> &str {
        "research_job_get"
    }
    fn description(&self) -> &str {
        "Get the finished bundle pointer for a job."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object",
            "properties":{"job_id":{"type":"string"}},
            "required":["job_id"]
        })
    }
    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let job_id = input
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: job_id".to_string()))?;
            self.mgr.get_bundle(job_id).await
        })
    }
}

struct ResearchJobCancelTool {
    mgr: Arc<JobManager>,
}

impl ResearchJobCancelTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for ResearchJobCancelTool {
    fn name(&self) -> &str {
        "research_job_cancel"
    }
    fn description(&self) -> &str {
        "Cancel a running job."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object",
            "properties":{"job_id":{"type":"string"}},
            "required":["job_id"]
        })
    }
    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let job_id = input
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: job_id".to_string()))?;
            self.mgr.cancel(job_id).await
        })
    }
}

struct ArtifactListTool {
    mgr: Arc<JobManager>,
}

impl ArtifactListTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for ArtifactListTool {
    fn name(&self) -> &str {
        "artifact_list"
    }
    fn description(&self) -> &str {
        "List artifacts for a job (from index.json)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "job_id":{"type":"string"},
                "prefix":{"type":"string"}
            },
            "required":["job_id"]
        })
    }
    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let job_id = input
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: job_id".to_string()))?;
            let prefix = input.get("prefix").and_then(|v| v.as_str()).unwrap_or("");
            self.mgr.artifact_list(job_id, prefix).await
        })
    }
}

struct ArtifactReadTool {
    mgr: Arc<JobManager>,
}

impl ArtifactReadTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for ArtifactReadTool {
    fn name(&self) -> &str {
        "artifact_read"
    }
    fn description(&self) -> &str {
        "Read an artifact file by job-relative path."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "job_id":{"type":"string"},
                "path":{"type":"string"}
            },
            "required":["job_id","path"]
        })
    }
    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let job_id = input
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: job_id".to_string()))?;
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: path".to_string()))?;
            self.mgr.artifact_read(job_id, path).await
        })
    }
}

/// Test helpers for v2 research backend.
pub mod testing {
    use super::*;

    /// Build a fake acquisition registry with static tools for offline tests.
    pub fn fake_acquisition_registry(tools: Vec<(&'static str, Value)>) -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        for (name, output) in tools {
            registry.register(Arc::new(StaticTool { name, output }));
        }
        registry
    }

    /// Build a backend registry for offline tests.
    pub fn backend_registry_for_tests(
        artifact_root: PathBuf,
        acquisition: ToolRegistry,
    ) -> ToolRegistry {
        let manager = Arc::new(JobManager::new(artifact_root, acquisition));
        backend_registry(manager)
    }

    struct StaticTool {
        name: &'static str,
        output: Value,
    }

    impl ToolDyn for StaticTool {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "static test tool"
        }
        fn input_schema(&self) -> Value {
            serde_json::json!({"type":"object"})
        }
        fn call(
            &self,
            _input: Value,
        ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
            let out = self.output.clone();
            Box::pin(async move { Ok(out) })
        }
    }
}
