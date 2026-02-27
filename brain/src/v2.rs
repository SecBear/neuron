//! Brain v2: structured research backend (async jobs + grounded bundles).

use base64::Engine;
use chrono::{DateTime, Utc};
use neuron_tool::{ToolDyn, ToolError, ToolRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

const BUNDLE_VERSION: &str = "0.1";
const INDEX_FILENAME: &str = "index.json";
const SPECPACK_DIR: &str = "specpack";
const SPECPACK_INDEX_PATH: &str = "specpack/SPECS.md";
const SPECPACK_MANIFEST_PATH: &str = "specpack/manifest.json";
const SPECPACK_DEFAULT_QUEUE_PATH: &str = "specpack/queue.json";
const SPECPACK_VERSION: &str = "0.1";
const SPECPACK_DEFAULT_LEDGER_PATH: &str = "ledger.json";
const SPECPACK_DEFAULT_CONFORMANCE_ROOT: &str = "conformance/";
const SPECPACK_DEFAULT_REQUIRED_SPEC_FILES: [&str; 2] = [
    "specs/05-edge-cases.md",
    "specs/06-testing-and-backpressure.md",
];

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackManifest {
    specpack_version: String,
    brain_version: String,
    job_id: String,
    produced_at: String,
    files: Vec<SpecPackManifestFile>,
    entrypoints: Vec<String>,
    roots: SpecPackManifestRoots,
    #[serde(default)]
    quality: Option<SpecPackManifestQuality>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackManifestFile {
    path: String,
    sha256: String,
    media_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackManifestRoots {
    specs_dir: String,
    queue_path: String,
    index_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackManifestQuality {
    ledger_path: String,
    conformance_root: String,
    #[serde(default)]
    required_spec_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackQueue {
    queue_version: String,
    job_id: String,
    created_at: String,
    #[serde(default)]
    tasks: Vec<SpecPackTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackLedger {
    ledger_version: String,
    job_id: String,
    created_at: String,
    #[serde(default)]
    targets: Vec<Value>,
    #[serde(default)]
    capabilities: Vec<SpecPackLedgerCapability>,
    #[serde(default)]
    gaps: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackLedgerCapability {
    id: String,
    domain: String,
    title: String,
    status: String,
    priority: i64,
    #[serde(default)]
    spec_refs: Vec<SpecPackSpecRef>,
    #[serde(default)]
    evidence: Vec<Value>,
    #[serde(default)]
    conformance_refs: Vec<SpecPackConformanceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackConformanceRef {
    path: String,
    kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackTask {
    id: String,
    title: String,
    kind: String,
    #[serde(default)]
    spec_refs: Vec<SpecPackSpecRef>,
    #[serde(default)]
    depends_on: Vec<String>,
    backpressure: SpecPackBackpressure,
    file_ownership: SpecPackFileOwnership,
    concurrency: SpecPackConcurrency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackSpecRef {
    path: String,
    #[serde(default)]
    anchor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackBackpressure {
    #[serde(default)]
    verify: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackFileOwnership {
    #[serde(default)]
    allow_globs: Vec<String>,
    #[serde(default)]
    deny_globs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpecPackConcurrency {
    #[serde(default)]
    group: Option<String>,
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

    fn specpack_dir(&self, job_id: &str) -> PathBuf {
        self.job_dir(job_id).join(SPECPACK_DIR)
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

    async fn ensure_job_exists(&self, job_id: &str) -> Result<(), ToolError> {
        let mut jobs = self.jobs.lock().await;
        if jobs.contains_key(job_id) {
            return Ok(());
        }
        if let Some(loaded) = self.try_load_job_record(job_id) {
            jobs.insert(job_id.to_string(), loaded);
            return Ok(());
        }
        Err(ToolError::NotFound(format!("job not found: {job_id}")))
    }

    async fn specpack_init(&self, job_id: &str) -> Result<Value, ToolError> {
        self.ensure_job_exists(job_id).await?;
        let specpack_dir = self.specpack_dir(job_id);
        std::fs::create_dir_all(specpack_dir.join("specs"))
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let specs_index = self.job_dir(job_id).join(SPECPACK_INDEX_PATH);
        if !specs_index.exists() {
            std::fs::write(&specs_index, "# SPECS\n")
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        }
        Ok(serde_json::json!({
            "job_id": job_id,
            "specpack_root": format!("{SPECPACK_DIR}/"),
            "index_path": SPECPACK_INDEX_PATH
        }))
    }

    async fn specpack_write_file(
        &self,
        job_id: &str,
        path: &str,
        encoding: &str,
        content: &str,
        media_type: &str,
    ) -> Result<Value, ToolError> {
        self.ensure_job_exists(job_id).await?;
        let rel = validate_specpack_job_path(path).map_err(ToolError::InvalidInput)?;
        let bytes = decode_content_bytes(encoding, content).map_err(ToolError::InvalidInput)?;
        let full = self.job_dir(job_id).join(&rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        }
        std::fs::write(&full, &bytes).map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(serde_json::json!({
            "path": rel.to_string_lossy(),
            "sha256": sha256_hex(&bytes),
            "media_type": media_type
        }))
    }

    async fn specpack_finalize(
        &self,
        job_id: &str,
        entrypoints: Vec<String>,
        queue_path: Option<String>,
    ) -> Result<Value, ToolError> {
        self.ensure_job_exists(job_id).await?;
        let job_dir = self.job_dir(job_id);
        let specpack_dir = self.specpack_dir(job_id);

        if !specpack_dir.exists() {
            return Err(ToolError::ExecutionFailed(
                "specpack directory missing; run specpack_init first".to_string(),
            ));
        }
        if !job_dir.join(SPECPACK_INDEX_PATH).exists() {
            return Err(ToolError::ExecutionFailed(format!(
                "missing required file: {SPECPACK_INDEX_PATH}"
            )));
        }
        if !specpack_dir.join("specs").exists() {
            return Err(ToolError::ExecutionFailed(
                "missing required directory: specpack/specs".to_string(),
            ));
        }

        ensure_specpack_quality_defaults_exist(&specpack_dir)
            .map_err(ToolError::ExecutionFailed)?;

        let manifest_path = job_dir.join(SPECPACK_MANIFEST_PATH);
        if manifest_path.exists() {
            let existing_text = std::fs::read_to_string(&manifest_path)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            validate_existing_manifest_drift(&job_dir, &existing_text)
                .map_err(|e| ToolError::ExecutionFailed(format!("specpack manifest drift: {e}")))?;
        }

        let queue_abs = queue_path.as_deref().unwrap_or(SPECPACK_DEFAULT_QUEUE_PATH);
        let queue_rel_job =
            validate_specpack_job_path(queue_abs).map_err(ToolError::InvalidInput)?;
        let queue_rel_specpack = normalize_specpack_relative_path(queue_rel_job.as_path())
            .map_err(ToolError::InvalidInput)?;
        validate_specpack_queue(&specpack_dir, &queue_rel_specpack, job_id)
            .map_err(ToolError::ExecutionFailed)?;

        let file_paths = collect_specpack_files(&specpack_dir)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        let mut files = Vec::<SpecPackManifestFile>::new();
        let mut file_set = HashSet::<String>::new();
        for path in file_paths {
            let full = specpack_dir.join(&path);
            let bytes =
                std::fs::read(&full).map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            let path_text = path.to_string_lossy().replace('\\', "/");
            file_set.insert(path_text.clone());
            files.push(SpecPackManifestFile {
                path: path_text,
                sha256: sha256_hex(&bytes),
                media_type: media_type_for_path(&full).to_string(),
            });
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let normalized_entrypoints =
            normalize_entrypoints(entrypoints).map_err(ToolError::InvalidInput)?;
        for entrypoint in &normalized_entrypoints {
            if !file_set.contains(entrypoint) {
                return Err(ToolError::ExecutionFailed(format!(
                    "entrypoint not found in specpack files: {entrypoint}"
                )));
            }
        }

        let manifest = SpecPackManifest {
            specpack_version: SPECPACK_VERSION.to_string(),
            brain_version: env!("CARGO_PKG_VERSION").to_string(),
            job_id: job_id.to_string(),
            produced_at: Utc::now().to_rfc3339(),
            files,
            entrypoints: normalized_entrypoints,
            roots: SpecPackManifestRoots {
                specs_dir: "specs/".to_string(),
                queue_path: queue_rel_specpack.to_string_lossy().replace('\\', "/"),
                index_path: "SPECS.md".to_string(),
            },
            quality: Some(SpecPackManifestQuality {
                ledger_path: SPECPACK_DEFAULT_LEDGER_PATH.to_string(),
                conformance_root: SPECPACK_DEFAULT_CONFORMANCE_ROOT.to_string(),
                required_spec_files: SPECPACK_DEFAULT_REQUIRED_SPEC_FILES
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            }),
        };
        validate_specpack_manifest(&job_dir, &manifest).map_err(ToolError::ExecutionFailed)?;
        validate_specpack_ledger(&job_dir, &manifest).map_err(ToolError::ExecutionFailed)?;

        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest)
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?,
        )
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        Ok(serde_json::json!({
            "job_id": job_id,
            "manifest_path": SPECPACK_MANIFEST_PATH,
            "file_count": manifest.files.len(),
        }))
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
    registry.register(Arc::new(SpecPackInitTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(SpecPackWriteFileTool::new(Arc::clone(&manager))));
    registry.register(Arc::new(SpecPackFinalizeTool::new(Arc::clone(&manager))));
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

fn validate_specpack_job_path(path: &str) -> Result<PathBuf, String> {
    let rel = validate_relative_path(path)?;
    match rel.components().next() {
        Some(Component::Normal(component)) if component == OsStr::new(SPECPACK_DIR) => Ok(rel),
        _ => Err(format!("path must be under `{SPECPACK_DIR}/`")),
    }
}

fn normalize_specpack_relative_path(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        return Err("path must be relative".to_string());
    }
    let mut components = path.components();
    let first = components
        .next()
        .ok_or_else(|| "path must not be empty".to_string())?;
    if first != Component::Normal(OsStr::new(SPECPACK_DIR)) {
        return Err(format!("path must start with `{SPECPACK_DIR}/`"));
    }
    let rel = components.as_path().to_path_buf();
    if rel.as_os_str().is_empty() {
        return Err("path must not be specpack root".to_string());
    }
    validate_relative_path(rel.to_string_lossy().as_ref())?;
    Ok(rel)
}

fn normalize_entrypoints(entrypoints: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for entrypoint in entrypoints {
        let input = validate_relative_path(&entrypoint)?;
        let rel = if input
            .components()
            .next()
            .is_some_and(|component| component == Component::Normal(OsStr::new(SPECPACK_DIR)))
        {
            normalize_specpack_relative_path(&input)?
        } else {
            input
        };
        normalized.push(rel.to_string_lossy().replace('\\', "/"));
    }
    Ok(normalized)
}

fn decode_content_bytes(encoding: &str, content: &str) -> Result<Vec<u8>, String> {
    match encoding {
        "utf-8" => Ok(content.as_bytes().to_vec()),
        "base64" => base64::engine::general_purpose::STANDARD
            .decode(content.as_bytes())
            .map_err(|e| format!("invalid base64 content: {e}")),
        other => Err(format!("unsupported encoding: {other}")),
    }
}

fn media_type_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("md") => "text/markdown",
        Some("json") => "application/json",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

fn collect_specpack_files(specpack_dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    fn walk(root: &Path, current: &Path, output: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, output)?;
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .map_err(std::io::Error::other)?
                .to_path_buf();
            if rel == Path::new("manifest.json") {
                continue;
            }
            output.push(rel);
        }
        Ok(())
    }

    let mut output = Vec::new();
    walk(specpack_dir, specpack_dir, &mut output)?;
    Ok(output)
}

fn validate_specpack_queue(
    specpack_dir: &Path,
    queue_path: &Path,
    job_id: &str,
) -> Result<(), String> {
    let full = specpack_dir.join(queue_path);
    if !full.exists() {
        return Err(format!(
            "missing required queue file: {}/{}",
            SPECPACK_DIR,
            queue_path.display()
        ));
    }
    let text = std::fs::read_to_string(&full).map_err(|e| e.to_string())?;
    let queue: SpecPackQueue = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let _ = parse_rfc3339_utc(&queue.created_at)?;
    if queue.job_id != job_id {
        return Err(format!(
            "queue job_id mismatch: expected {job_id}, got {}",
            queue.job_id
        ));
    }
    let mut task_ids = HashSet::<String>::new();
    for task in &queue.tasks {
        if task.id.trim().is_empty() {
            return Err("queue task id must be non-empty".to_string());
        }
        if task.kind == "impl" && task.backpressure.verify.is_empty() {
            return Err(format!(
                "queue task `{}` kind=impl must include at least one backpressure.verify command",
                task.id
            ));
        }
        if !task_ids.insert(task.id.clone()) {
            return Err(format!("duplicate queue task id: {}", task.id));
        }
        for spec_ref in &task.spec_refs {
            let rel = validate_relative_path(&spec_ref.path)?;
            if rel
                .components()
                .next()
                .is_some_and(|component| component == Component::Normal(OsStr::new(SPECPACK_DIR)))
            {
                return Err(format!(
                    "queue spec_refs must be specpack-relative, got: {}",
                    spec_ref.path
                ));
            }
            if !spec_ref.path.starts_with("specs/") {
                return Err(format!(
                    "queue spec_refs path must be under specs/: {}",
                    spec_ref.path
                ));
            }
            if !specpack_dir.join(&rel).exists() {
                return Err(format!(
                    "queue spec_refs path missing from specpack: {}",
                    spec_ref.path
                ));
            }
        }
    }
    for task in &queue.tasks {
        for dep in &task.depends_on {
            if !task_ids.contains(dep) {
                return Err(format!(
                    "queue task `{}` depends on unknown task `{dep}`",
                    task.id
                ));
            }
        }
    }
    Ok(())
}

fn validate_specpack_manifest(job_dir: &Path, manifest: &SpecPackManifest) -> Result<(), String> {
    let _ = parse_rfc3339_utc(&manifest.produced_at)?;
    let specpack_dir = job_dir.join(SPECPACK_DIR);
    let mut file_paths = HashSet::<String>::new();
    for file in &manifest.files {
        if file.path.trim().is_empty() {
            return Err("manifest file path must be non-empty".to_string());
        }
        if !file_paths.insert(file.path.clone()) {
            return Err(format!("duplicate manifest file path: {}", file.path));
        }
        let rel = validate_relative_path(&file.path)?;
        let full = specpack_dir.join(&rel);
        let bytes = std::fs::read(&full)
            .map_err(|e| format!("missing manifest file `{}`: {e}", file.path))?;
        let computed = sha256_hex(&bytes);
        if computed != file.sha256 {
            return Err(format!(
                "hash mismatch for `{}`: expected {}, got {}",
                file.path, file.sha256, computed
            ));
        }
    }
    for entrypoint in &manifest.entrypoints {
        if !file_paths.contains(entrypoint) {
            return Err(format!("manifest entrypoint not found: {entrypoint}"));
        }
    }
    if manifest.roots.specs_dir != "specs/" {
        return Err("manifest roots.specs_dir must be `specs/`".to_string());
    }
    if !file_paths.contains(&manifest.roots.index_path) {
        return Err(format!(
            "manifest roots.index_path not found in files: {}",
            manifest.roots.index_path
        ));
    }
    if !file_paths.contains(&manifest.roots.queue_path) {
        return Err(format!(
            "manifest roots.queue_path not found in files: {}",
            manifest.roots.queue_path
        ));
    }

    let quality = manifest
        .quality
        .as_ref()
        .ok_or_else(|| "manifest quality missing".to_string())?;
    let ledger_rel = validate_relative_path(&quality.ledger_path)?;
    let ledger_rel_text = ledger_rel.to_string_lossy().replace('\\', "/");
    if !file_paths.contains(&ledger_rel_text) {
        return Err(format!(
            "manifest quality.ledger_path not found in files: {}",
            quality.ledger_path
        ));
    }

    let conformance_root = normalize_dir_prefix(&quality.conformance_root)?;
    let conformance_readme = format!("{conformance_root}README.md");
    let conformance_verify = format!("{conformance_root}verify");
    if !file_paths.contains(&conformance_readme) {
        return Err(format!(
            "missing required conformance file in manifest: {conformance_readme}"
        ));
    }
    if !file_paths.contains(&conformance_verify) {
        return Err(format!(
            "missing required conformance file in manifest: {conformance_verify}"
        ));
    }

    for required in &quality.required_spec_files {
        if !required.starts_with("specs/") {
            return Err(format!(
                "manifest quality.required_spec_files must be under specs/: {required}"
            ));
        }
        if !file_paths.contains(required) {
            return Err(format!(
                "manifest required spec file missing from files: {required}"
            ));
        }
    }
    Ok(())
}

fn normalize_dir_prefix(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("directory prefix must be non-empty".to_string());
    }
    let rel = validate_relative_path(trimmed)?;
    let text = rel.to_string_lossy().replace('\\', "/");
    if text.is_empty() {
        return Err("directory prefix must be non-empty".to_string());
    }
    if text.ends_with('/') {
        Ok(text)
    } else {
        Ok(format!("{text}/"))
    }
}

fn ensure_specpack_quality_defaults_exist(specpack_dir: &Path) -> Result<(), String> {
    let required_files = [
        PathBuf::from(SPECPACK_DEFAULT_LEDGER_PATH),
        PathBuf::from("conformance/README.md"),
        PathBuf::from("conformance/verify"),
        PathBuf::from(SPECPACK_DEFAULT_REQUIRED_SPEC_FILES[0]),
        PathBuf::from(SPECPACK_DEFAULT_REQUIRED_SPEC_FILES[1]),
    ];

    for rel in required_files {
        let full = specpack_dir.join(&rel);
        if !full.exists() {
            return Err(format!(
                "missing required file: {SPECPACK_DIR}/{}",
                rel.display()
            ));
        }
    }
    Ok(())
}

fn validate_existing_manifest_drift(job_dir: &Path, manifest_text: &str) -> Result<(), String> {
    let parsed: Value = serde_json::from_str(manifest_text).map_err(|e| e.to_string())?;
    let files = parsed
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "manifest drift check: missing files[]".to_string())?;

    let specpack_dir = job_dir.join(SPECPACK_DIR);
    let mut seen_paths = HashSet::<String>::new();
    for file in files {
        let path = file
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "manifest drift check: file missing path".to_string())?;
        let sha256 = file
            .get("sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "manifest drift check: file missing sha256".to_string())?;
        if !seen_paths.insert(path.to_string()) {
            return Err(format!("manifest drift check: duplicate file path: {path}"));
        }
        let rel = validate_relative_path(path)?;
        let full = specpack_dir.join(&rel);
        let bytes = std::fs::read(&full)
            .map_err(|e| format!("manifest drift check: missing file `{path}`: {e}"))?;
        let computed = sha256_hex(&bytes);
        if computed != sha256 {
            return Err(format!(
                "manifest drift check: hash mismatch for `{path}`: expected {sha256}, got {computed}"
            ));
        }
    }
    Ok(())
}

fn validate_specpack_ledger(job_dir: &Path, manifest: &SpecPackManifest) -> Result<(), String> {
    let quality = manifest
        .quality
        .as_ref()
        .ok_or_else(|| "manifest quality missing".to_string())?;

    let ledger_path = validate_relative_path(&quality.ledger_path)?;
    let ledger_full = job_dir.join(SPECPACK_DIR).join(&ledger_path);
    let text = std::fs::read_to_string(&ledger_full).map_err(|e| e.to_string())?;
    let ledger: SpecPackLedger = serde_json::from_str(&text).map_err(|e| e.to_string())?;

    let _ = parse_rfc3339_utc(&ledger.created_at)?;
    if ledger.job_id != manifest.job_id {
        return Err(format!(
            "ledger job_id mismatch: expected {}, got {}",
            manifest.job_id, ledger.job_id
        ));
    }

    let conformance_root = normalize_dir_prefix(&quality.conformance_root)?;
    let manifest_paths: HashSet<String> = manifest.files.iter().map(|f| f.path.clone()).collect();

    let mut capability_ids = HashSet::<String>::new();
    for capability in &ledger.capabilities {
        if capability.id.trim().is_empty() {
            return Err("ledger capability id must be non-empty".to_string());
        }
        if !capability_ids.insert(capability.id.clone()) {
            return Err(format!("duplicate ledger capability id: {}", capability.id));
        }
        match capability.status.as_str() {
            "unknown" | "specified" | "implemented" | "verified" => {}
            other => return Err(format!("invalid ledger capability status: {other}")),
        }
        for spec_ref in &capability.spec_refs {
            let rel = validate_relative_path(&spec_ref.path)?;
            if rel
                .components()
                .next()
                .is_some_and(|component| component == Component::Normal(OsStr::new(SPECPACK_DIR)))
            {
                return Err(format!(
                    "ledger spec_refs must be specpack-relative, got: {}",
                    spec_ref.path
                ));
            }
            if !spec_ref.path.starts_with("specs/") {
                return Err(format!(
                    "ledger spec_refs path must be under specs/: {}",
                    spec_ref.path
                ));
            }
            if !manifest_paths.contains(&spec_ref.path) {
                return Err(format!(
                    "ledger spec_refs path missing from manifest files: {}",
                    spec_ref.path
                ));
            }
        }

        if capability.status == "verified" && capability.conformance_refs.is_empty() {
            return Err(format!(
                "ledger capability `{}` status=verified must include conformance_refs[]",
                capability.id
            ));
        }
        for cref in &capability.conformance_refs {
            let rel = validate_relative_path(&cref.path)?;
            if rel
                .components()
                .next()
                .is_some_and(|component| component == Component::Normal(OsStr::new(SPECPACK_DIR)))
            {
                return Err(format!(
                    "ledger conformance_refs must be specpack-relative, got: {}",
                    cref.path
                ));
            }
            if !cref.path.starts_with(&conformance_root) {
                return Err(format!(
                    "ledger conformance_refs must be under conformance root `{conformance_root}`, got: {}",
                    cref.path
                ));
            }
            if !manifest_paths.contains(&cref.path) {
                return Err(format!(
                    "ledger conformance_refs path missing from manifest files: {}",
                    cref.path
                ));
            }
            match cref.kind.as_str() {
                "golden" | "test" | "trace" | "matrix" => {}
                other => return Err(format!("invalid conformance_refs kind: {other}")),
            }
        }
    }

    Ok(())
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

struct SpecPackInitTool {
    mgr: Arc<JobManager>,
}

impl SpecPackInitTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for SpecPackInitTool {
    fn name(&self) -> &str {
        "specpack_init"
    }
    fn description(&self) -> &str {
        "Initialize specpack directories for an existing research job."
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
            self.mgr.specpack_init(job_id).await
        })
    }
}

struct SpecPackWriteFileTool {
    mgr: Arc<JobManager>,
}

impl SpecPackWriteFileTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for SpecPackWriteFileTool {
    fn name(&self) -> &str {
        "specpack_write_file"
    }
    fn description(&self) -> &str {
        "Write one file under specpack/ for an existing job."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "job_id":{"type":"string"},
                "path":{"type":"string"},
                "encoding":{"type":"string"},
                "content":{"type":"string"},
                "media_type":{"type":"string"}
            },
            "required":["job_id","path","encoding","content","media_type"]
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
            let encoding = input
                .get("encoding")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: encoding".to_string()))?;
            let content = input
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: content".to_string()))?;
            let media_type = input
                .get("media_type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::InvalidInput("missing field: media_type".to_string()))?;
            self.mgr
                .specpack_write_file(job_id, path, encoding, content, media_type)
                .await
        })
    }
}

struct SpecPackFinalizeTool {
    mgr: Arc<JobManager>,
}

impl SpecPackFinalizeTool {
    fn new(mgr: Arc<JobManager>) -> Self {
        Self { mgr }
    }
}

impl ToolDyn for SpecPackFinalizeTool {
    fn name(&self) -> &str {
        "specpack_finalize"
    }
    fn description(&self) -> &str {
        "Validate and write specpack/manifest.json for a job."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type":"object",
            "properties":{
                "job_id":{"type":"string"},
                "entrypoints":{"type":"array","items":{"type":"string"}},
                "queue_path":{"type":"string"}
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
            let entrypoints = input
                .get("entrypoints")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|value| {
                    value.as_str().map(str::to_string).ok_or_else(|| {
                        ToolError::InvalidInput("entrypoints must be strings".to_string())
                    })
                })
                .collect::<Result<Vec<String>, ToolError>>()?;
            let queue_path = input
                .get("queue_path")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            self.mgr
                .specpack_finalize(job_id, entrypoints, queue_path)
                .await
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
