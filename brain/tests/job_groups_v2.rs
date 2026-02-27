use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

async fn wait_for_terminal_status(registry: &neuron_tool::ToolRegistry, job_id: &str) {
    for _ in 0..200 {
        let status = registry
            .get("research_job_status")
            .expect("research_job_status exists")
            .call(json!({"job_id": job_id}))
            .await
            .expect("status ok");
        match status.get("status").and_then(|v| v.as_str()) {
            Some("succeeded") | Some("failed") | Some("canceled") => return,
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("job did not reach terminal status in time");
}

async fn wait_for_group_terminal(registry: &neuron_tool::ToolRegistry, group_id: &str) {
    for _ in 0..200 {
        let status = registry
            .get("research_group_status")
            .expect("research_group_status exists")
            .call(json!({"group_id": group_id}))
            .await
            .expect("group status ok");
        match status.get("status").and_then(|v| v.as_str()) {
            Some("succeeded") | Some("failed") | Some("canceled") => return,
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("group did not reach terminal status in time");
}

fn copy_dir_all(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("create dst dir");
    for entry in std::fs::read_dir(src).expect("read_dir src") {
        let entry = entry.expect("dir entry");
        let ty = entry.file_type().expect("file_type");
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path);
        } else {
            std::fs::copy(entry.path(), &dst_path).expect("copy file");
        }
    }
}

#[tokio::test]
async fn v2_group_job_fans_out_and_merges_landscape_bundle() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");

    let acquisition = brain::v2::testing::fake_acquisition_registry(vec![(
        "web_search",
        json!({"results":[{"title":"Example","url":"https://example.invalid","snippet":"alpha"}]}),
    )]);
    let registry = brain::v2::testing::backend_registry_for_tests(artifact_root, acquisition);

    let start = registry
        .get("research_group_start")
        .expect("research_group_start exists")
        .call(json!({
            "intent":"landscape",
            "targets":["AlphaCo","BetaCo"],
            "constraints":{},
            "tool_policy":{}
        }))
        .await
        .expect("start ok");

    let group_id = start
        .get("group_id")
        .and_then(|v| v.as_str())
        .expect("group_id")
        .to_string();

    let jobs = start
        .get("jobs")
        .and_then(|v| v.as_array())
        .expect("jobs array");
    assert_eq!(jobs.len(), 2);

    wait_for_group_terminal(&registry, &group_id).await;
    let status = registry
        .get("research_group_status")
        .expect("research_group_status exists")
        .call(json!({"group_id": group_id.clone()}))
        .await
        .expect("status ok");
    assert_eq!(
        status.get("status").and_then(|v| v.as_str()),
        Some("succeeded")
    );

    let landscape_job_id = status
        .get("landscape_job_id")
        .and_then(|v| v.as_str())
        .expect("landscape_job_id");

    wait_for_terminal_status(&registry, landscape_job_id).await;

    let got = registry
        .get("research_job_get")
        .expect("research_job_get exists")
        .call(json!({"job_id": landscape_job_id}))
        .await
        .expect("get ok");
    assert_eq!(
        got.get("status").and_then(|v| v.as_str()),
        Some("succeeded")
    );

    let index_path = temp
        .path()
        .join(".brain")
        .join("artifacts")
        .join(landscape_job_id)
        .join("index.json");
    let index_text = std::fs::read_to_string(index_path).expect("index exists");
    let index: serde_json::Value = serde_json::from_str(&index_text).expect("index parse");

    let merged_targets = index
        .get("coverage")
        .and_then(|v| v.get("targets"))
        .and_then(|v| v.as_array())
        .expect("coverage.targets array");
    assert!(merged_targets.iter().any(|t| t.as_str() == Some("AlphaCo")));
    assert!(merged_targets.iter().any(|t| t.as_str() == Some("BetaCo")));
}

#[tokio::test]
async fn v2_merge_is_deterministic_from_fixtures() {
    let temp = tempfile::tempdir().expect("tempdir");
    let artifact_root = temp.path().join(".brain").join("artifacts");

    let fixture_root: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("merge");
    copy_dir_all(&fixture_root.join("job_a"), &artifact_root.join("job_a"));
    copy_dir_all(&fixture_root.join("job_b"), &artifact_root.join("job_b"));

    let registry = brain::v2::testing::backend_registry_for_tests(
        artifact_root,
        brain::v2::testing::fake_acquisition_registry(vec![]),
    );

    let merge1 = registry
        .get("research_job_merge")
        .expect("research_job_merge exists")
        .call(json!({"job_ids":["job_a","job_b"], "intent":"Fixture merge"}))
        .await
        .expect("merge1 start ok");
    let job1 = merge1
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    wait_for_terminal_status(&registry, &job1).await;

    let merge2 = registry
        .get("research_job_merge")
        .expect("research_job_merge exists")
        .call(json!({"job_ids":["job_a","job_b"], "intent":"Fixture merge"}))
        .await
        .expect("merge2 start ok");
    let job2 = merge2
        .get("job_id")
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_string();
    wait_for_terminal_status(&registry, &job2).await;

    let index1_text = std::fs::read_to_string(
        temp.path()
            .join(".brain/artifacts")
            .join(&job1)
            .join("index.json"),
    )
    .expect("index1 exists");
    let index2_text = std::fs::read_to_string(
        temp.path()
            .join(".brain/artifacts")
            .join(&job2)
            .join("index.json"),
    )
    .expect("index2 exists");
    let index1: serde_json::Value = serde_json::from_str(&index1_text).expect("index1 parse");
    let index2: serde_json::Value = serde_json::from_str(&index2_text).expect("index2 parse");

    assert_eq!(
        index1
            .get("coverage")
            .and_then(|v| v.get("targets"))
            .and_then(|v| v.as_array())
            .expect("targets array"),
        index2
            .get("coverage")
            .and_then(|v| v.get("targets"))
            .and_then(|v| v.as_array())
            .expect("targets array"),
    );
    assert_eq!(
        index1
            .get("coverage")
            .and_then(|v| v.get("gaps"))
            .and_then(|v| v.as_array())
            .expect("gaps array"),
        index2
            .get("coverage")
            .and_then(|v| v.get("gaps"))
            .and_then(|v| v.as_array())
            .expect("gaps array"),
    );
    assert_eq!(
        index1
            .get("next_steps")
            .and_then(|v| v.as_array())
            .expect("next_steps array"),
        index2
            .get("next_steps")
            .and_then(|v| v.as_array())
            .expect("next_steps array"),
    );

    let targets = index1
        .get("coverage")
        .and_then(|v| v.get("targets"))
        .and_then(|v| v.as_array())
        .expect("targets array");
    let gaps = index1
        .get("coverage")
        .and_then(|v| v.get("gaps"))
        .and_then(|v| v.as_array())
        .expect("gaps array");
    assert_eq!(targets, &vec![json!("AlphaCo"), json!("BetaCo")]);
    assert_eq!(gaps, &vec![json!("api"), json!("pricing")]);
}
