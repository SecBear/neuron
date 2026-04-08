use async_trait::async_trait;
use layer0::content::Content;
use layer0::dispatch_context::DispatchContext;
use layer0::id::{DispatchId, OperatorId, SessionId};
use layer0::operator::{Operator, OperatorInput, Outcome, TerminalOutcome};
use skg_op_compute_runtime::{ComputeRuntime, ExecutionProfile, ExecutionReport};
use skg_turn::test_utils::{make_text_response, TestProvider};
use std::sync::Arc;

fn test_ctx() -> DispatchContext {
    DispatchContext::new(DispatchId::new("test"), OperatorId::new("compute"))
}

fn simple_input(text: &str) -> OperatorInput {
    OperatorInput::new(Content::text(text), layer0::operator::TriggerType::User)
}

// Dummy runtime that echoes the code as stdout
#[derive(Default)]
struct EchoRuntime;

#[async_trait]
impl ComputeRuntime for EchoRuntime {
    async fn exec(
        &self,
        _session_id: &SessionId,
        code: &str,
        _profile: &ExecutionProfile,
    ) -> Result<ExecutionReport, skg_op_compute_runtime::ComputeError> {
        Ok(ExecutionReport {
            stdout: code.to_string(),
            ..ExecutionReport::default()
        })
    }

    async fn reset(&self, _session_id: &SessionId) -> Result<(), skg_op_compute_runtime::ComputeError> {
        Ok(())
    }

    async fn close(&self, _session_id: &SessionId) -> Result<(), skg_op_compute_runtime::ComputeError> {
        Ok(())
    }

    async fn inspect(&self, session_id: &SessionId) -> Result<skg_op_compute_runtime::ComputeSession, skg_op_compute_runtime::ComputeError> {
        Ok(skg_op_compute_runtime::ComputeSession::new(session_id.clone(), "python"))
    }
}

#[tokio::test]
async fn compute_operator_runs_code_and_returns_stdout() {
    // Provider returns a fenced python block
    let provider = TestProvider::with_responses(vec![make_text_response(
        "Please execute the following.\n```python\nprint('ok')\n```\n",
    )]);
    let runtime = Arc::new(EchoRuntime::default());
    let op = skg_op_compute_runtime::operator::ComputeOperator::new(provider, runtime);

    let out = op.execute(simple_input("run"), &test_ctx()).await.unwrap();
    assert_eq!(out.outcome, Outcome::Terminal { terminal: TerminalOutcome::Completed });
    // Our EchoRuntime returns the code body as stdout
    assert_eq!(out.message.as_text().unwrap().trim_end(), "print('ok')");
}

// Runtime that returns a structured final_result
struct FinalResultRuntime;

#[async_trait]
impl ComputeRuntime for FinalResultRuntime {
    async fn exec(
        &self,
        _session_id: &SessionId,
        _code: &str,
        _profile: &ExecutionProfile,
    ) -> Result<ExecutionReport, skg_op_compute_runtime::ComputeError> {
        Ok(ExecutionReport {
            final_result: Some(serde_json::json!({"answer": 42})),
            ..ExecutionReport::default()
        })
    }

    async fn reset(&self, _session_id: &SessionId) -> Result<(), skg_op_compute_runtime::ComputeError> {
        Ok(())
    }

    async fn close(&self, _session_id: &SessionId) -> Result<(), skg_op_compute_runtime::ComputeError> {
        Ok(())
    }

    async fn inspect(&self, session_id: &SessionId) -> Result<skg_op_compute_runtime::ComputeSession, skg_op_compute_runtime::ComputeError> {
        Ok(skg_op_compute_runtime::ComputeSession::new(session_id.clone(), "python"))
    }
}

#[tokio::test]
async fn compute_operator_prefers_final_result_over_stdout() {
    let provider = TestProvider::with_responses(vec![make_text_response(
        "```python\nfinal({'answer': 42})\n```",
    )]);
    let runtime = Arc::new(FinalResultRuntime);
    let op = skg_op_compute_runtime::operator::ComputeOperator::new(provider, runtime);

    let out = op.execute(simple_input("question"), &test_ctx()).await.unwrap();
    // Expect a data block
    if let Content::Blocks(blocks) = out.message {
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            layer0::content::ContentBlock::Data { data, .. } => {
                assert_eq!(data["answer"], 42);
            }
            other => panic!("unexpected content block: {:?}", other),
        }
    } else {
        panic!("expected blocks content");
    }
}
