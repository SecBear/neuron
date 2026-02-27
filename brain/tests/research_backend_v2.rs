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
