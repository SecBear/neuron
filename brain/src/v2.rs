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

const BUNDLE_VERSION: &str = "0.1";
const INDEX_FILENAME: &str = "index.json";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleIndex {
    bundle_version: String,
    brain_version: String,
    job: BundleJob,
    artifacts: Vec<BundleArtifact>,
    claims: Vec<BundleClaim>,
    coverage: BundleCoverage,
    next_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleJob {
    id: String,
    created_at: String,
    status: JobStatus,
    inputs: BundleInputs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleInputs {
    intent: String,
    #[serde(default)]
    constraints: Value,
    #[serde(default)]
    targets: Vec<Value>,
    #[serde(default)]
    tool_policy: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleArtifact {
    path: String,
    sha256: String,
    media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    retrieved_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ClaimKind {
    Fact,
    Assumption,
    DesignChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleClaim {
    id: String,
    kind: ClaimKind,
    statement: String,
    #[serde(default)]
    evidence: Vec<BundleEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleEvidence {
    artifact_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locator: Option<Value>,
    retrieved_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BundleCoverage {
    #[serde(default)]
    targets: Vec<Value>,
    #[serde(default)]
    gaps: Vec<String>,
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

    async fn list_jobs(&self, status: Option<JobStatus>) -> Result<Value, ToolError> {
        let jobs = self.jobs.lock().await;
        let mut items = Vec::<Value>::new();
        for (job_id, entry) in jobs.iter() {
            if let Some(filter) = &status {
                if &entry.status != filter {
                    continue;
                }
            }
            items.push(serde_json::json!({
                "job_id": job_id,
                "status": entry.status,
                "created_at": entry.created_at.to_rfc3339()
            }));
        }
        Ok(serde_json::json!({ "jobs": items }))
    }

    async fn artifact_list(&self, job_id: &str, prefix: &str) -> Result<Value, ToolError> {
        let prefix = prefix.to_string();
        let index = self.read_index_typed(job_id).await?;
        let filtered: Vec<Value> = index
            .artifacts
            .into_iter()
            .filter_map(|a| {
                if a.path.starts_with(&prefix) {
                    Some(serde_json::json!({
                        "path": a.path,
                        "sha256": a.sha256
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

    async fn read_index_typed(&self, job_id: &str) -> Result<BundleIndex, ToolError> {
        let path = self.job_dir(job_id).join(INDEX_FILENAME);
        let content = std::fs::read_to_string(&path)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let parsed: BundleIndex = serde_json::from_str(&content)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        validate_bundle_index(&self.job_dir(job_id), &parsed)
            .map_err(ToolError::ExecutionFailed)?;
        Ok(parsed)
    }
}

/// Build a ToolRegistry exposing Brain v2 MCP tool surface.
pub fn backend_registry(manager: Arc<JobManager>) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ResearchJobStartTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(ResearchJobStatusTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(ResearchJobGetTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(ResearchJobListTool::new(Arc::clone(&manager))));
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

    let retrieved_at = Utc::now().to_rfc3339();
    let mut artifacts = Vec::<BundleArtifact>::new();

    let primary_source_path: String;

    let bundle_inputs = BundleInputs {
        intent: inputs.intent.clone(),
        constraints: inputs.constraints.clone(),
        targets: inputs.targets.clone(),
        tool_policy: inputs.tool_policy.clone(),
    };

    // Acquisition strategy:
    // 1) Prefer deep-research async tools when available (start/status/get).
    // 2) Fallback to web_search if available.
    // 3) Otherwise write the intent as the only source snapshot.
    if acquisition.get("deep_research_start").is_some()
        && acquisition.get("deep_research_status").is_some()
        && acquisition.get("deep_research_get").is_some()
    {
        let start_tool = acquisition
            .get("deep_research_start")
            .ok_or_else(|| "missing deep_research_start".to_string())?;
        let status_tool = acquisition
            .get("deep_research_status")
            .ok_or_else(|| "missing deep_research_status".to_string())?;
        let get_tool = acquisition
            .get("deep_research_get")
            .ok_or_else(|| "missing deep_research_get".to_string())?;

        let start_out = start_tool
            .call(serde_json::json!({
                "intent": bundle_inputs.intent.clone(),
                "query": bundle_inputs.intent.clone(),
                "constraints": bundle_inputs.constraints.clone(),
                "targets": bundle_inputs.targets.clone(),
                "tool_policy": bundle_inputs.tool_policy.clone()
            }))
            .await
            .map_err(|e| e.to_string())?;

        let deep_id = start_out
            .get("job_id")
            .or_else(|| start_out.get("id"))
            .or_else(|| start_out.get("task_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("deep_research")
            .to_string();

        artifacts.push(write_json_artifact(
            &job_dir,
            Path::new("sources/deep_research_start.json"),
            &start_out,
            "application/json",
            Some(retrieved_at.clone()),
            None,
        )?);

        let mut final_status = Value::Null;
        for _ in 0..50 {
            let status_out = status_tool
                .call(serde_json::json!({ "job_id": deep_id.clone(), "id": deep_id.clone() }))
                .await
                .map_err(|e| e.to_string())?;
            final_status = status_out.clone();
            if is_deep_research_done(&status_out) {
                break;
            }
        }

        artifacts.push(write_json_artifact(
            &job_dir,
            Path::new("sources/deep_research_status.json"),
            &final_status,
            "application/json",
            Some(retrieved_at.clone()),
            None,
        )?);

        if !is_deep_research_done(&final_status) {
            return Err("deep research did not complete in time".to_string());
        }

        let get_out = get_tool
            .call(serde_json::json!({ "job_id": deep_id.clone(), "id": deep_id.clone() }))
            .await
            .map_err(|e| e.to_string())?;

        let rel_path = PathBuf::from("sources/deep_research_get.json");
        artifacts.push(write_json_artifact(
            &job_dir,
            &rel_path,
            &get_out,
            "application/json",
            Some(retrieved_at.clone()),
            None,
        )?);
        primary_source_path = rel_path.to_string_lossy().to_string();
    } else if let Some(tool) = acquisition.get("web_search") {
        let out = tool
            .call(serde_json::json!({ "query": inputs.intent }))
            .await
            .map_err(|e| e.to_string())?;

        let rel_path = PathBuf::from("sources/web_search.json");
        artifacts.push(write_json_artifact(
            &job_dir,
            &rel_path,
            &out,
            "application/json",
            Some(retrieved_at.clone()),
            None,
        )?);

        if let Some(fetch) = acquisition.get("web_fetch") {
            let maybe_url = out
                .get("results")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|first| first.get("url"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(url) = maybe_url {
                let fetched = fetch
                    .call(serde_json::json!({ "url": url.clone() }))
                    .await
                    .map_err(|e| e.to_string())?;
                let fetch_path = PathBuf::from("sources/web_fetch_0.json");
                artifacts.push(write_json_artifact(
                    &job_dir,
                    &fetch_path,
                    &fetched,
                    "application/json",
                    Some(retrieved_at.clone()),
                    Some(url),
                )?);
                primary_source_path = fetch_path.to_string_lossy().to_string();
            } else {
                primary_source_path = rel_path.to_string_lossy().to_string();
            }
        } else {
            primary_source_path = rel_path.to_string_lossy().to_string();
        }
    } else {
        let rel_path = PathBuf::from("sources/intent.txt");
        artifacts.push(write_bytes_artifact(
            &job_dir,
            &rel_path,
            inputs.intent.as_bytes(),
            "text/plain",
            Some(retrieved_at.clone()),
            None,
        )?);
        primary_source_path = rel_path.to_string_lossy().to_string();
    }

    let index = serde_json::json!({
        "bundle_version": BUNDLE_VERSION,
        "brain_version": env!("CARGO_PKG_VERSION"),
        "job": {
            "id": job_id,
            "created_at": created_at.to_rfc3339(),
            "status": "succeeded",
            "inputs": bundle_inputs
        },
        "artifacts": artifacts,
        "claims": [serde_json::json!({
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
        })],
        "coverage": {
            "targets": [],
            "gaps": []
        },
        "next_steps": []
    });

    std::fs::write(
        job_dir.join(INDEX_FILENAME),
        serde_json::to_string_pretty(&index).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // Validation gate: index.json must parse into typed structs and pass invariants.
    let index_text =
        std::fs::read_to_string(job_dir.join(INDEX_FILENAME)).map_err(|e| e.to_string())?;
    let parsed: BundleIndex = serde_json::from_str(&index_text).map_err(|e| e.to_string())?;
    validate_bundle_index(&job_dir, &parsed)?;

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

fn validate_bundle_index(job_dir: &Path, index: &BundleIndex) -> Result<(), String> {
    if index.bundle_version.trim().is_empty() {
        return Err("bundle_version must be non-empty".to_string());
    }
    if index.brain_version.trim().is_empty() {
        return Err("brain_version must be non-empty".to_string());
    }

    let _ = parse_rfc3339_utc(&index.job.created_at)?;

    let mut artifact_paths = HashMap::<String, BundleArtifact>::new();
    for artifact in &index.artifacts {
        validate_relative_path(&artifact.path)?;
        if artifact_paths.contains_key(&artifact.path) {
            return Err(format!("duplicate artifact path: {}", artifact.path));
        }
        let full = job_dir.join(&artifact.path);
        let bytes = std::fs::read(&full).map_err(|e| {
            format!(
                "artifact file missing or unreadable: {}: {}",
                artifact.path, e
            )
        })?;
        let computed = sha256_hex(&bytes);
        if computed != artifact.sha256 {
            return Err(format!(
                "artifact sha256 mismatch: {} expected={} got={}",
                artifact.path, artifact.sha256, computed
            ));
        }
        if let Some(ts) = &artifact.retrieved_at {
            let _ = parse_rfc3339_utc(ts)?;
        }
        artifact_paths.insert(artifact.path.clone(), artifact.clone());
    }

    for claim in &index.claims {
        if claim.kind == ClaimKind::Fact && claim.evidence.is_empty() {
            return Err("fact claim missing evidence".to_string());
        }
        for ev in &claim.evidence {
            validate_relative_path(&ev.artifact_path)?;
            if !artifact_paths.contains_key(&ev.artifact_path) {
                return Err(format!(
                    "evidence artifact_path not present in artifacts[]: {}",
                    ev.artifact_path
                ));
            }
            let _ = parse_rfc3339_utc(&ev.retrieved_at)?;
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

fn write_json_artifact(
    job_dir: &Path,
    rel_path: &Path,
    json: &Value,
    media_type: &str,
    retrieved_at: Option<String>,
    source_url: Option<String>,
) -> Result<BundleArtifact, String> {
    let pretty = serde_json::to_string_pretty(json).map_err(|e| e.to_string())?;
    write_bytes_artifact(
        job_dir,
        rel_path,
        pretty.as_bytes(),
        media_type,
        retrieved_at,
        source_url,
    )
}

fn write_bytes_artifact(
    job_dir: &Path,
    rel_path: &Path,
    bytes: &[u8],
    media_type: &str,
    retrieved_at: Option<String>,
    source_url: Option<String>,
) -> Result<BundleArtifact, String> {
    let rel_path = rel_path
        .to_str()
        .ok_or_else(|| "artifact path must be valid utf-8".to_string())?;
    let rel = validate_relative_path(rel_path)?;
    let full_path = job_dir.join(&rel);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&full_path, bytes).map_err(|e| e.to_string())?;
    let sha = sha256_hex(bytes);
    Ok(BundleArtifact {
        path: rel.to_string_lossy().to_string(),
        sha256: sha,
        media_type: media_type.to_string(),
        retrieved_at,
        source_url,
    })
}

fn is_deep_research_done(status: &Value) -> bool {
    if let Some(done) = status.get("done").and_then(|v| v.as_bool()) {
        return done;
    }
    matches!(
        status.get("status").and_then(|v| v.as_str()),
        Some("succeeded") | Some("completed") | Some("done")
    )
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

struct ResearchJobListTool {
    mgr: Arc<JobManager>,
}

impl ResearchJobListTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for ResearchJobListTool {
    fn name(&self) -> &str {
        "research_job_list"
    }
    fn description(&self) -> &str {
        "List known research jobs (including persisted jobs after restart)."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "status":{"type":["string","null"]}
            }
        })
    }
    fn call(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let status = match input.get("status") {
                None | Some(Value::Null) => None,
                Some(v) => {
                    let parsed: JobStatus = serde_json::from_value(v.clone())
                        .map_err(|e| ToolError::InvalidInput(format!("invalid status: {e}")))?;
                    Some(parsed)
                }
            };
            self.mgr.list_jobs(status).await
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
