use base64::Engine as _;
use neuron_tool::{ToolDyn, ToolError, ToolRegistry};
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

async fn wait_for_terminal_status(registry: &ToolRegistry, job_id: &str) -> serde_json::Value {
    for _ in 0..200 {
        let status = registry
            .get("research_job_status")
            .expect("status tool exists")
            .call(json!({"job_id": job_id}))
            .await
            .expect("status ok");
        match status.get("status").and_then(|v| v.as_str()) {
            Some("succeeded") | Some("failed") | Some("canceled") => return status,
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("job did not reach terminal status in time");
}

async fn specpack_write(
    registry: &ToolRegistry,
    job_id: &str,
    path: &str,
    content: &str,
    media_type: &str,
) {
    registry
        .get("specpack_write_file")
        .expect("specpack_write_file exists")
        .call(json!({
            "job_id": job_id,
            "path": path,
            "encoding": "utf-8",
            "content": content,
            "media_type": media_type
        }))
        .await
        .unwrap_or_else(|e| panic!("write {path} failed: {e}"));
}

async fn specpack_write_minimal_quality_bundle(registry: &ToolRegistry, job_id: &str) {
    let ledger = json!({
        "ledger_version": "0.1",
        "job_id": job_id,
        "created_at": "2026-02-27T00:00:00Z",
        "targets": [],
        "capabilities": [{
            "id": "cap_overview",
            "domain": "docs",
            "title": "Overview exists",
            "status": "specified",
            "priority": 1,
            "spec_refs": [{"path":"specs/00-overview.md","anchor": null}],
            "evidence": []
        }],
        "gaps": []
    });

    specpack_write(
        registry,
        job_id,
        "specpack/ledger.json",
        &serde_json::to_string_pretty(&ledger).expect("ledger json"),
        "application/json",
    )
    .await;

    specpack_write(
        registry,
        job_id,
        "specpack/conformance/README.md",
        "# Conformance\n\nRun `./verify`.\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        registry,
        job_id,
        "specpack/conformance/verify",
        "echo ok\n",
        "text/plain",
    )
    .await;

    specpack_write(
        registry,
        job_id,
        "specpack/specs/05-edge-cases.md",
        "# Edge Cases\n\n- TBD\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        registry,
        job_id,
        "specpack/specs/06-testing-and-backpressure.md",
        "# Testing\n\nBackpressure lives here.\n",
        "text/markdown",
    )
    .await;

    let feature_map = json!({
        "feature_map_version": "0.1",
        "job_id": job_id,
        "produced_at": "2026-02-27T00:00:00Z",
        "capabilities": [{
            "capability_id": "cap_overview",
            "spec_refs": [],
            "code_refs": [],
            "trace_refs": [],
            "slice_refs": []
        }]
    });
    specpack_write(
        registry,
        job_id,
        "specpack/analysis/feature_map.json",
        &serde_json::to_string_pretty(&feature_map).expect("feature_map json"),
        "application/json",
    )
    .await;
}

#[derive(Debug, Clone)]
struct RecordedCall {
    tool: String,
}

struct RecordingSequenceTool {
    name: &'static str,
    outputs: Mutex<Vec<serde_json::Value>>,
    calls: Arc<Mutex<Vec<RecordedCall>>>,
}

impl RecordingSequenceTool {
    fn new(
        name: &'static str,
        outputs: Vec<serde_json::Value>,
        calls: Arc<Mutex<Vec<RecordedCall>>>,
    ) -> Self {
        Self {
            name,
            outputs: Mutex::new(outputs),
            calls,
        }
    }
}

impl ToolDyn for RecordingSequenceTool {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        "recording sequence test tool"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object"})
    }

    fn call(
        &self,
        _input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>> {
        Box::pin(async move {
            {
                let mut calls = self.calls.lock().await;
                calls.push(RecordedCall {
                    tool: self.name.to_string(),
                });
            }

            let mut outputs = self.outputs.lock().await;
            if outputs.is_empty() {
                return Ok(serde_json::Value::Null);
            }
            if outputs.len() == 1 {
                return Ok(outputs[0].clone());
            }
            Ok(outputs.remove(0))
        })
    }
}

#[tokio::test]
async fn v2_job_writes_bundle_and_artifacts_and_can_be_inspected() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");

    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![(
        "web_search",
        json!({"results":[{"title":"Example","url":"https://example.invalid","snippet":"alpha"}]}),
    )]);

    let registry =
        brain::v2::testing::backend_registry_for_tests(artifact_root.clone(), acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"test query","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    // Poll status until succeeded.
    for _ in 0..50 {
        let status = registry
            .get("research_job_status")
            .expect("status tool exists")
            .call(json!({"job_id": job_id.clone()}))
            .await
            .expect("status ok");
        if status.get("status").and_then(|v| v.as_str()) == Some("succeeded") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let got = registry
        .get("research_job_get")
        .expect("get tool exists")
        .call(json!({"job_id": job_id.clone()}))
        .await
        .expect("get ok");

    let bundle = got.get("bundle").expect("bundle");
    assert_eq!(
        bundle.get("index_path").and_then(|v| v.as_str()),
        Some("index.json")
    );
    assert_eq!(
        bundle.get("findings_path").and_then(|v| v.as_str()),
        Some("findings.md")
    );

    // Artifact list/read.
    let list = registry
        .get("artifact_list")
        .expect("artifact_list tool exists")
        .call(json!({"job_id": job_id.clone(), "prefix":"sources/"}))
        .await
        .expect("list ok");
    let artifacts = list
        .get("artifacts")
        .and_then(|v| v.as_array())
        .expect("artifacts array");
    assert!(!artifacts.is_empty());

    let first_path = artifacts[0]
        .get("path")
        .and_then(|v| v.as_str())
        .expect("path");
    let read = registry
        .get("artifact_read")
        .expect("artifact_read tool exists")
        .call(json!({"job_id": job_id, "path": first_path}))
        .await
        .expect("read ok");
    assert_eq!(read.get("path").and_then(|v| v.as_str()), Some(first_path));
    assert!(
        read.get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .len()
            > 0
    );
    assert!(
        read.get("sha256")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .len()
            > 0
    );
}

#[tokio::test]
async fn v2_index_json_includes_bundle_and_brain_versions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");

    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![(
        "web_search",
        json!({"results":[{"title":"Example","url":"https://example.invalid","snippet":"alpha"}]}),
    )]);

    let registry =
        brain::v2::testing::backend_registry_for_tests(artifact_root.clone(), acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"version check","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let terminal = wait_for_terminal_status(&registry, &job_id).await;
    assert_eq!(
        terminal.get("status").and_then(|v| v.as_str()),
        Some("succeeded")
    );

    let index_path = artifact_root.join(&job_id).join("index.json");
    let index_text = std::fs::read_to_string(index_path).expect("index.json exists");
    let index: serde_json::Value = serde_json::from_str(&index_text).expect("index.json parse");

    assert!(
        index
            .get("bundle_version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .len()
            > 0
    );
    assert_eq!(
        index.get("brain_version").and_then(|v| v.as_str()),
        Some(env!("CARGO_PKG_VERSION"))
    );
}

#[tokio::test]
async fn v2_prefers_deep_research_async_roles_when_available() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");

    let calls: Arc<Mutex<Vec<RecordedCall>>> = Arc::new(Mutex::new(Vec::new()));
    let mut acquisition = ToolRegistry::new();
    acquisition.register(Arc::new(RecordingSequenceTool::new(
        "deep_research_start",
        vec![json!({"job_id":"dr1"})],
        Arc::clone(&calls),
    )));
    acquisition.register(Arc::new(RecordingSequenceTool::new(
        "deep_research_status",
        vec![json!({"status":"running"}), json!({"status":"succeeded"})],
        Arc::clone(&calls),
    )));
    acquisition.register(Arc::new(RecordingSequenceTool::new(
        "deep_research_get",
        vec![json!({"result":"ok"})],
        Arc::clone(&calls),
    )));
    acquisition.register(Arc::new(RecordingSequenceTool::new(
        "web_search",
        vec![json!({"results":[]})],
        Arc::clone(&calls),
    )));

    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"deep research","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let terminal = wait_for_terminal_status(&registry, &job_id).await;
    assert_eq!(
        terminal.get("status").and_then(|v| v.as_str()),
        Some("succeeded")
    );

    let calls = calls.lock().await;
    assert!(calls.iter().any(|c| c.tool == "deep_research_start"));
    assert!(calls.iter().any(|c| c.tool == "deep_research_status"));
    assert!(calls.iter().any(|c| c.tool == "deep_research_get"));
    assert!(!calls.iter().any(|c| c.tool == "web_search"));
}

#[tokio::test]
async fn v2_deep_research_claims_are_ignored_and_do_not_break_bundle_contract() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");

    // Deep research returns invalid claim shapes (missing required fields),
    // which Brain MUST ignore to keep the bundle contract stable.
    let calls: Arc<Mutex<Vec<RecordedCall>>> = Arc::new(Mutex::new(Vec::new()));
    let mut acquisition = ToolRegistry::new();
    acquisition.register(Arc::new(RecordingSequenceTool::new(
        "deep_research_start",
        vec![json!({"job_id":"dr1"})],
        Arc::clone(&calls),
    )));
    acquisition.register(Arc::new(RecordingSequenceTool::new(
        "deep_research_status",
        vec![json!({"status":"succeeded"})],
        Arc::clone(&calls),
    )));
    acquisition.register(Arc::new(RecordingSequenceTool::new(
        "deep_research_get",
        vec![json!({"claims":[{"kind":"fact"}]})],
        Arc::clone(&calls),
    )));

    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"invalid index","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let terminal = wait_for_terminal_status(&registry, &job_id).await;
    assert_eq!(
        terminal.get("status").and_then(|v| v.as_str()),
        Some("succeeded")
    );
}

#[tokio::test]
async fn v2_jobs_persist_across_manager_restart() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");

    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![(
        "web_search",
        json!({"results":[{"title":"Example","url":"https://example.invalid","snippet":"alpha"}]}),
    )]);

    let registry1 =
        brain::v2::testing::backend_registry_for_tests(artifact_root.clone(), acquisition);
    let start = registry1
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"persist test","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    for _ in 0..50 {
        let status = registry1
            .get("research_job_status")
            .expect("status tool exists")
            .call(json!({"job_id": job_id.clone()}))
            .await
            .expect("status ok");
        if status.get("status").and_then(|v| v.as_str()) == Some("succeeded") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let registry2 = brain::v2::testing::backend_registry_for_tests(
        artifact_root,
        brain::v2::testing::fake_acquisition_registry(vec![]),
    );

    let status2 = registry2
        .get("research_job_status")
        .expect("status tool exists")
        .call(json!({"job_id": job_id.clone()}))
        .await
        .expect("status ok");
    assert_eq!(
        status2.get("status").and_then(|v| v.as_str()),
        Some("succeeded")
    );

    let got2 = registry2
        .get("research_job_get")
        .expect("get tool exists")
        .call(json!({"job_id": job_id.clone()}))
        .await
        .expect("get ok");
    assert_eq!(
        got2.get("status").and_then(|v| v.as_str()),
        Some("succeeded")
    );
}

#[tokio::test]
async fn v2_research_job_list_discovers_jobs_across_restart() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");

    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![(
        "web_search",
        json!({"results":[{"title":"Example","url":"https://example.invalid","snippet":"alpha"}]}),
    )]);

    let registry1 =
        brain::v2::testing::backend_registry_for_tests(artifact_root.clone(), acquisition);
    let start = registry1
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"list test","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let terminal = wait_for_terminal_status(&registry1, &job_id).await;
    assert_eq!(
        terminal.get("status").and_then(|v| v.as_str()),
        Some("succeeded")
    );

    let registry2 = brain::v2::testing::backend_registry_for_tests(
        artifact_root,
        brain::v2::testing::fake_acquisition_registry(vec![]),
    );

    let listed = registry2
        .get("research_job_list")
        .expect("research_job_list exists")
        .call(json!({"status":"succeeded"}))
        .await
        .expect("list ok");
    let jobs = listed
        .get("jobs")
        .and_then(|v| v.as_array())
        .expect("jobs array");
    assert!(
        jobs.iter()
            .any(|j| j.get("job_id").and_then(|v| v.as_str()) == Some(job_id.as_str()))
    );
}

#[tokio::test]
async fn v2_evidence_pointers_reference_existing_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");

    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![(
        "web_search",
        json!({"results":[{"title":"Example","url":"https://example.invalid","snippet":"alpha"}]}),
    )]);

    let registry =
        brain::v2::testing::backend_registry_for_tests(artifact_root.clone(), acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"evidence check","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    let terminal = wait_for_terminal_status(&registry, &job_id).await;
    assert_eq!(
        terminal.get("status").and_then(|v| v.as_str()),
        Some("succeeded")
    );

    let job_dir = artifact_root.join(&job_id);
    let index_text = std::fs::read_to_string(job_dir.join("index.json")).expect("index.json");
    let index: serde_json::Value = serde_json::from_str(&index_text).expect("index parse");

    let artifacts = index
        .get("artifacts")
        .and_then(|v| v.as_array())
        .expect("artifacts array");
    let mut artifact_paths = std::collections::HashSet::<String>::new();
    for a in artifacts {
        if let Some(path) = a.get("path").and_then(|v| v.as_str()) {
            artifact_paths.insert(path.to_string());
        }
    }

    let claims = index
        .get("claims")
        .and_then(|v| v.as_array())
        .expect("claims");
    for claim in claims {
        if claim.get("kind").and_then(|v| v.as_str()) != Some("fact") {
            continue;
        }
        let evidence = claim
            .get("evidence")
            .and_then(|v| v.as_array())
            .expect("fact evidence");
        assert!(!evidence.is_empty());
        for ev in evidence {
            let path = ev
                .get("artifact_path")
                .and_then(|v| v.as_str())
                .expect("artifact_path");
            assert!(artifact_paths.contains(path));
            assert!(job_dir.join(path).exists());
        }
    }
}

#[tokio::test]
async fn v2_artifact_read_rejects_path_traversal() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"x","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();

    // Immediate traversal attempt should be rejected even if job not finished.
    let err = registry
        .get("artifact_read")
        .expect("artifact_read tool exists")
        .call(json!({"job_id": job_id, "path":"../secrets.txt"}))
        .await
        .expect_err("reject traversal");
    let _ = err;
}

#[tokio::test]
async fn v2_specpack_finalize_writes_manifest_for_valid_queue() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry =
        brain::v2::testing::backend_registry_for_tests(artifact_root.clone(), acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"specpack","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let _terminal = wait_for_terminal_status(&registry, &job_id).await;

    registry
        .get("specpack_init")
        .expect("specpack_init exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect("specpack_init ok");

    specpack_write(
        &registry,
        &job_id,
        "specpack/SPECS.md",
        "# SPECS\n\n- [Overview](specs/00-overview.md)\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/00-overview.md",
        "# Overview\n\nhello\n",
        "text/markdown",
    )
    .await;
    specpack_write_minimal_quality_bundle(&registry, &job_id).await;

    specpack_write(
        &registry,
        &job_id,
        "specpack/queue.json",
        &serde_json::to_string_pretty(&json!({
            "queue_version":"0.1",
            "job_id": job_id,
            "created_at":"2026-02-27T00:00:00Z",
            "tasks":[{
                "id":"task_1",
                "title":"Implement overview",
                "kind":"spec",
                "spec_refs":[{"path":"specs/00-overview.md","anchor":null}],
                "depends_on":[],
                "backpressure":{"verify":["nix develop -c cargo test -p brain"]},
                "file_ownership":{"allow_globs":["brain/**"],"deny_globs":[]},
                "concurrency":{"group":null}
            }]
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    let finalized = registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({
            "job_id": job_id,
            "entrypoints": ["specs/00-overview.md"]
        }))
        .await
        .expect("finalize ok");
    assert_eq!(
        finalized.get("manifest_path").and_then(|v| v.as_str()),
        Some("specpack/manifest.json")
    );

    let manifest_path = artifact_root
        .join(
            finalized
                .get("job_id")
                .and_then(|v| v.as_str())
                .expect("job_id"),
        )
        .join("specpack/manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path).expect("manifest"))
            .expect("manifest json");
    let files = manifest
        .get("files")
        .and_then(|v| v.as_array())
        .expect("files");
    assert!(
        files
            .iter()
            .any(|f| { f.get("path").and_then(|v| v.as_str()) == Some("specs/00-overview.md") })
    );
}

#[tokio::test]
async fn v2_specpack_finalize_rejects_manifest_drift() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"specpack drift","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let _terminal = wait_for_terminal_status(&registry, &job_id).await;

    specpack_write(
        &registry,
        &job_id,
        "specpack/SPECS.md",
        "# SPECS\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/00-overview.md",
        "# Overview\n",
        "text/markdown",
    )
    .await;
    specpack_write_minimal_quality_bundle(&registry, &job_id).await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/queue.json",
        &serde_json::to_string_pretty(&json!({
            "queue_version":"0.1",
            "job_id": job_id,
            "created_at":"2026-02-27T00:00:00Z",
            "tasks":[]
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect("first finalize");

    registry
        .get("specpack_write_file")
        .expect("specpack_write_file exists")
        .call(json!({
            "job_id": job_id,
            "path": "specpack/specs/00-overview.md",
            "encoding": "utf-8",
            "content": "# Overview\n\nchanged\n",
            "media_type": "text/markdown"
        }))
        .await
        .expect("mutate spec");

    let err = registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect_err("drift must fail");
    assert!(
        err.to_string().contains("drift"),
        "expected drift error, got: {err}"
    );
}

#[tokio::test]
async fn v2_specpack_finalize_rejects_queue_path_traversal_refs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(
            json!({"intent":"specpack path safety","constraints":{},"targets":[],"tool_policy":{}}),
        )
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let _terminal = wait_for_terminal_status(&registry, &job_id).await;

    specpack_write(
        &registry,
        &job_id,
        "specpack/SPECS.md",
        "# SPECS\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/00-overview.md",
        "# Overview\n",
        "text/markdown",
    )
    .await;
    specpack_write_minimal_quality_bundle(&registry, &job_id).await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/queue.json",
        &serde_json::to_string_pretty(&json!({
            "queue_version":"0.1",
            "job_id": job_id,
            "created_at":"2026-02-27T00:00:00Z",
            "tasks":[{
                "id":"task_unsafe",
                "title":"Unsafe",
                "kind":"spec",
                "spec_refs":[{"path":"../outside.md","anchor":null}],
                "depends_on":[],
                "backpressure":{"verify":["true"]},
                "file_ownership":{"allow_globs":["**"],"deny_globs":[]},
                "concurrency":{"group":null}
            }]
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    let err = registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect_err("unsafe ref must fail");
    assert!(
        err.to_string().contains("path"),
        "expected path error, got: {err}"
    );
}

#[tokio::test]
async fn v2_specpack_finalize_rejects_missing_ledger_json() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"specpack missing ledger","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let _terminal = wait_for_terminal_status(&registry, &job_id).await;

    registry
        .get("specpack_init")
        .expect("specpack_init exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect("specpack_init ok");

    specpack_write(
        &registry,
        &job_id,
        "specpack/SPECS.md",
        "# SPECS\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/00-overview.md",
        "# Overview\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/conformance/README.md",
        "# Conformance\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/conformance/verify",
        "echo ok\n",
        "text/plain",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/05-edge-cases.md",
        "# Edge Cases\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/06-testing-and-backpressure.md",
        "# Testing\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/queue.json",
        &serde_json::to_string_pretty(&json!({
            "queue_version":"0.1",
            "job_id": job_id,
            "created_at":"2026-02-27T00:00:00Z",
            "tasks":[]
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    let err = registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect_err("missing ledger must fail");
    assert!(
        err.to_string().contains("ledger"),
        "expected ledger error, got: {err}"
    );
}

#[tokio::test]
async fn v2_specpack_finalize_rejects_ledger_spec_refs_missing_from_specpack() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"specpack bad ledger refs","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let _terminal = wait_for_terminal_status(&registry, &job_id).await;

    registry
        .get("specpack_init")
        .expect("specpack_init exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect("specpack_init ok");

    specpack_write(
        &registry,
        &job_id,
        "specpack/SPECS.md",
        "# SPECS\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/00-overview.md",
        "# Overview\n",
        "text/markdown",
    )
    .await;

    let bad_ledger = json!({
        "ledger_version": "0.1",
        "job_id": job_id,
        "created_at": "2026-02-27T00:00:00Z",
        "targets": [],
        "capabilities": [{
            "id": "cap_missing",
            "domain": "docs",
            "title": "Refers to missing spec",
            "status": "specified",
            "priority": 1,
            "spec_refs": [{"path":"specs/does-not-exist.md","anchor": null}],
            "evidence": []
        }],
        "gaps": []
    });
    specpack_write(
        &registry,
        &job_id,
        "specpack/ledger.json",
        &serde_json::to_string_pretty(&bad_ledger).expect("ledger json"),
        "application/json",
    )
    .await;

    specpack_write(
        &registry,
        &job_id,
        "specpack/conformance/README.md",
        "# Conformance\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/conformance/verify",
        "echo ok\n",
        "text/plain",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/05-edge-cases.md",
        "# Edge Cases\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/06-testing-and-backpressure.md",
        "# Testing\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/analysis/feature_map.json",
        &serde_json::to_string_pretty(&json!({
            "feature_map_version": "0.1",
            "job_id": job_id,
            "produced_at": "2026-02-27T00:00:00Z",
            "capabilities": [{
                "capability_id": "cap_missing",
                "spec_refs": [],
                "code_refs": [],
                "trace_refs": [],
                "slice_refs": []
            }]
        }))
        .expect("json"),
        "application/json",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/queue.json",
        &serde_json::to_string_pretty(&json!({
            "queue_version":"0.1",
            "job_id": job_id,
            "created_at":"2026-02-27T00:00:00Z",
            "tasks":[]
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    let err = registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect_err("bad ledger refs must fail");
    assert!(
        err.to_string().contains("spec"),
        "expected spec ref error, got: {err}"
    );
}

#[tokio::test]
async fn v2_specpack_finalize_rejects_impl_task_missing_verify_commands() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"specpack missing verify","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let _terminal = wait_for_terminal_status(&registry, &job_id).await;

    registry
        .get("specpack_init")
        .expect("specpack_init exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect("specpack_init ok");

    specpack_write(
        &registry,
        &job_id,
        "specpack/SPECS.md",
        "# SPECS\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/00-overview.md",
        "# Overview\n",
        "text/markdown",
    )
    .await;
    specpack_write_minimal_quality_bundle(&registry, &job_id).await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/queue.json",
        &serde_json::to_string_pretty(&json!({
            "queue_version":"0.1",
            "job_id": job_id,
            "created_at":"2026-02-27T00:00:00Z",
            "tasks":[{
                "id":"task_impl",
                "title":"Implement something",
                "kind":"impl",
                "spec_refs":[{"path":"specs/00-overview.md","anchor":null}],
                "depends_on":[],
                "backpressure":{"verify":[]},
                "file_ownership":{"allow_globs":["**"],"deny_globs":[]},
                "concurrency":{"group":null}
            }]
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    let err = registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect_err("impl task without verify must fail");
    assert!(
        err.to_string().contains("verify"),
        "expected verify error, got: {err}"
    );
}

// ── traceability: feature_map validation tests ────────────────────────────────

#[tokio::test]
async fn v2_specpack_finalize_rejects_missing_feature_map() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(
            json!({"intent":"missing feature_map","constraints":{},"targets":[],"tool_policy":{}}),
        )
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let _terminal = wait_for_terminal_status(&registry, &job_id).await;

    registry
        .get("specpack_init")
        .expect("specpack_init exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect("specpack_init ok");

    // Write all required quality files EXCEPT analysis/feature_map.json
    specpack_write(
        &registry,
        &job_id,
        "specpack/SPECS.md",
        "# SPECS\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/00-overview.md",
        "# Overview\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/ledger.json",
        &serde_json::to_string_pretty(&json!({
            "ledger_version": "0.1", "job_id": job_id,
            "created_at": "2026-02-27T00:00:00Z",
            "targets": [], "capabilities": [], "gaps": []
        }))
        .expect("json"),
        "application/json",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/conformance/README.md",
        "# Conformance\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/conformance/verify",
        "echo ok\n",
        "text/plain",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/05-edge-cases.md",
        "# Edge Cases\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/06-testing-and-backpressure.md",
        "# Testing\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/queue.json",
        &serde_json::to_string_pretty(&json!({
            "queue_version": "0.1", "job_id": job_id,
            "created_at": "2026-02-27T00:00:00Z", "tasks": []
        }))
        .expect("json"),
        "application/json",
    )
    .await;
    // Note: analysis/feature_map.json is NOT written

    let err = registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect_err("missing feature_map must fail");
    assert!(
        err.to_string().contains("feature_map") || err.to_string().contains("analysis"),
        "expected feature_map error, got: {err}"
    );
}

#[tokio::test]
async fn v2_specpack_finalize_rejects_unknown_capability_id_in_feature_map() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(json!({"intent":"unknown capability_id","constraints":{},"targets":[],"tool_policy":{}}))
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let _terminal = wait_for_terminal_status(&registry, &job_id).await;

    registry
        .get("specpack_init")
        .expect("specpack_init exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect("specpack_init ok");

    specpack_write(
        &registry,
        &job_id,
        "specpack/SPECS.md",
        "# SPECS\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/00-overview.md",
        "# Overview\n",
        "text/markdown",
    )
    .await;
    specpack_write_minimal_quality_bundle(&registry, &job_id).await;

    // Override feature_map with one referencing a capability_id not in ledger
    specpack_write(
        &registry,
        &job_id,
        "specpack/analysis/feature_map.json",
        &serde_json::to_string_pretty(&json!({
            "feature_map_version": "0.1",
            "job_id": job_id,
            "produced_at": "2026-02-27T00:00:00Z",
            "capabilities": [{
                "capability_id": "cap_DOES_NOT_EXIST",
                "spec_refs": [], "code_refs": [], "trace_refs": [], "slice_refs": []
            }]
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    specpack_write(
        &registry,
        &job_id,
        "specpack/queue.json",
        &serde_json::to_string_pretty(&json!({
            "queue_version": "0.1", "job_id": job_id,
            "created_at": "2026-02-27T00:00:00Z", "tasks": []
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    let err = registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect_err("unknown capability_id must fail");
    assert!(
        err.to_string().contains("capability_id") || err.to_string().contains("not found"),
        "expected capability_id error, got: {err}"
    );
}

#[tokio::test]
async fn v2_specpack_finalize_rejects_missing_artifact_ref_in_feature_map() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![(
        "web_search",
        json!({"results":[{"title":"T","url":"https://example.invalid","snippet":"x"}]}),
    )]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_job_start")
        .expect("start tool exists")
        .call(
            json!({"intent":"missing artifact ref","constraints":{},"targets":[],"tool_policy":{}}),
        )
        .await
        .expect("start ok");
    let job_id = start
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    let _terminal = wait_for_terminal_status(&registry, &job_id).await;

    registry
        .get("specpack_init")
        .expect("specpack_init exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect("specpack_init ok");

    specpack_write(
        &registry,
        &job_id,
        "specpack/SPECS.md",
        "# SPECS\n",
        "text/markdown",
    )
    .await;
    specpack_write(
        &registry,
        &job_id,
        "specpack/specs/00-overview.md",
        "# Overview\n",
        "text/markdown",
    )
    .await;
    specpack_write_minimal_quality_bundle(&registry, &job_id).await;

    // Override feature_map with a code_ref pointing to a nonexistent artifact
    specpack_write(
        &registry,
        &job_id,
        "specpack/analysis/feature_map.json",
        &serde_json::to_string_pretty(&json!({
            "feature_map_version": "0.1",
            "job_id": job_id,
            "produced_at": "2026-02-27T00:00:00Z",
            "capabilities": [{
                "capability_id": "cap_overview",
                "spec_refs": [],
                "code_refs": [{"artifact_path": "sources/nonexistent_artifact_xyzzy.json"}],
                "trace_refs": [],
                "slice_refs": []
            }]
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    specpack_write(
        &registry,
        &job_id,
        "specpack/queue.json",
        &serde_json::to_string_pretty(&json!({
            "queue_version": "0.1", "job_id": job_id,
            "created_at": "2026-02-27T00:00:00Z", "tasks": []
        }))
        .expect("json"),
        "application/json",
    )
    .await;

    let err = registry
        .get("specpack_finalize")
        .expect("specpack_finalize exists")
        .call(json!({"job_id": job_id}))
        .await
        .expect_err("missing artifact ref must fail");
    assert!(
        err.to_string().contains("artifact") || err.to_string().contains("nonexistent"),
        "expected artifact ref error, got: {err}"
    );
}

// ── artifact_import / artifact_write tests ────────────────────────────────────

fn make_artifact_registry() -> (neuron_tool::ToolRegistry, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");
    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);
    (registry, temp)
}

#[tokio::test]
async fn artifact_import_utf8_writes_bytes_and_returns_sha256() {
    let (registry, _tmp) = make_artifact_registry();
    let job_id = "test-job-import-utf8";
    let content = "hello, world\n";

    let result = registry
        .get("artifact_import")
        .expect("artifact_import tool exists")
        .call(json!({
            "job_id": job_id,
            "path": "sources/hello.txt",
            "encoding": "utf-8",
            "content": content,
            "media_type": "text/plain"
        }))
        .await
        .expect("artifact_import succeeded");

    let returned_path = result["path"].as_str().expect("path field");
    assert_eq!(returned_path, "sources/hello.txt");

    let sha256 = result["sha256"].as_str().expect("sha256 field");
    // Compute expected sha256 over the raw bytes
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(content.as_bytes());
    let expected = hex::encode(hasher.finalize());
    assert_eq!(sha256, expected);
}

#[tokio::test]
async fn artifact_import_base64_decodes_and_returns_sha256() {
    let (registry, _tmp) = make_artifact_registry();
    let job_id = "test-job-import-b64";
    let raw = b"\x00\x01\x02\x03binary";
    let b64 = base64::engine::general_purpose::STANDARD.encode(raw);

    let result = registry
        .get("artifact_import")
        .expect("artifact_import tool exists")
        .call(json!({
            "job_id": job_id,
            "path": "sources/blob.bin",
            "encoding": "base64",
            "content": b64,
            "media_type": "application/octet-stream"
        }))
        .await
        .expect("artifact_import base64 succeeded");

    let sha256 = result["sha256"].as_str().expect("sha256 field");
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(raw);
    let expected = hex::encode(hasher.finalize());
    assert_eq!(sha256, expected);
}

#[tokio::test]
async fn artifact_import_rejects_traversal_path() {
    let (registry, _tmp) = make_artifact_registry();

    let err = registry
        .get("artifact_import")
        .expect("artifact_import tool exists")
        .call(json!({
            "job_id": "any-job",
            "path": "../escape.txt",
            "encoding": "utf-8",
            "content": "bad",
            "media_type": "text/plain"
        }))
        .await
        .expect_err("traversal must be rejected");
    assert!(
        err.to_string().contains(".."),
        "expected traversal error, got: {err}"
    );
}

#[tokio::test]
async fn artifact_import_rejects_absolute_path() {
    let (registry, _tmp) = make_artifact_registry();

    let err = registry
        .get("artifact_import")
        .expect("artifact_import tool exists")
        .call(json!({
            "job_id": "any-job",
            "path": "/etc/passwd",
            "encoding": "utf-8",
            "content": "bad",
            "media_type": "text/plain"
        }))
        .await
        .expect_err("absolute path must be rejected");
    assert!(
        err.to_string().contains("absolute") || err.to_string().contains("not"),
        "expected absolute path error, got: {err}"
    );
}

#[tokio::test]
async fn artifact_write_overwrites_and_hash_changes() {
    let (registry, _tmp) = make_artifact_registry();
    let job_id = "test-job-write-overwrite";
    let path = "derived/spec.md";

    let r1 = registry
        .get("artifact_write")
        .expect("artifact_write tool exists")
        .call(json!({
            "job_id": job_id,
            "path": path,
            "encoding": "utf-8",
            "content": "version 1\n",
            "media_type": "text/markdown"
        }))
        .await
        .expect("first write succeeded");

    let sha1 = r1["sha256"].as_str().expect("sha256 v1").to_string();

    let r2 = registry
        .get("artifact_write")
        .expect("artifact_write tool exists")
        .call(json!({
            "job_id": job_id,
            "path": path,
            "encoding": "utf-8",
            "content": "version 2 — updated\n",
            "media_type": "text/markdown"
        }))
        .await
        .expect("second write succeeded");

    let sha2 = r2["sha256"].as_str().expect("sha256 v2").to_string();
    assert_ne!(sha1, sha2, "hash must change when content changes");
}

#[tokio::test]
async fn artifact_write_rejects_traversal_path() {
    let (registry, _tmp) = make_artifact_registry();

    let err = registry
        .get("artifact_write")
        .expect("artifact_write tool exists")
        .call(json!({
            "job_id": "any-job",
            "path": "../../etc/crontab",
            "encoding": "utf-8",
            "content": "bad",
            "media_type": "text/plain"
        }))
        .await
        .expect_err("traversal must be rejected");
    assert!(
        err.to_string().contains(".."),
        "expected traversal error, got: {err}"
    );
}
