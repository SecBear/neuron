use serde_json::json;
use std::fs;

#[tokio::test]
async fn offline_controller_worker_synthesis_and_mcp_loading() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mcp_path = temp.path().join(".mcp.json");
    fs::write(
        &mcp_path,
        r#"
{
  "mcpServers": {
    "local-search": {
      "command": "fake-server",
      "args": ["--stdio"]
    }
  },
  "x-brain": {
    "allowlist": ["sonnet_summarize", "repo_search"],
    "denylist": ["codex_generate_patch"]
  }
}
"#,
    )
    .expect("write mcp");

    let run = brain::testing::run_offline_brain(brain::testing::OfflineBrainScenario {
        user_message: "Summarize the notes and answer clearly".into(),
        controller_responses: vec![
            brain::testing::tool_use_response(
                "tu_1",
                "sonnet_summarize",
                json!({"text":"alpha beta gamma", "goal":"quick summary"}),
            ),
            brain::testing::text_response("Final answer: alpha beta summary."),
        ],
        worker_responses: vec![brain::testing::text_response(
            r#"{"summary":"alpha beta summary","key_points":["alpha","beta"]}"#,
        )],
        mcp_path: mcp_path.clone(),
        fake_mcp_tools: vec![brain::testing::fake_tool(
            "repo_search",
            "Search repository",
            json!({"hits":[{"path":"README.md"}]}),
        )],
    })
    .await
    .expect("brain run succeeds");

    assert_eq!(run.final_answer, "Final answer: alpha beta summary.");
    assert!(run.tool_calls.contains(&"sonnet_summarize".to_string()));
    assert_eq!(
        run.worker_json,
        json!({"summary":"alpha beta summary","key_points":["alpha","beta"]})
    );
    assert!(run.exposed_tools.contains(&"repo_search".to_string()));
    assert!(run.exposed_tools.contains(&"sonnet_summarize".to_string()));
    assert!(
        !run.exposed_tools
            .contains(&"codex_generate_patch".to_string())
    );
}
