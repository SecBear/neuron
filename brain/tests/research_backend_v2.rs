use serde_json::json;
use std::time::Duration;

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
