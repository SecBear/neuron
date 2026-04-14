use std::sync::Arc;

use layer0::content::Content;
use layer0::dispatch_context::DispatchContext;
use layer0::id::{DispatchId, OperatorId, SessionId};
use layer0::operator::{Operator, OperatorInput, Outcome, TerminalOutcome};
use skg_op_compute_runtime::python::LocalPythonBackend;
use skg_op_compute_runtime::runtime::InMemoryComputeRuntime;
use skg_turn::test_utils::{TestProvider, make_text_response};

#[tokio::main]
async fn main() {
    // Scripted provider that returns a Python code block calling final({...})
    let provider = TestProvider::with_responses(vec![make_text_response(
        r#"Please produce the result by calling final(...) in Python.
```python
final({'answer': 42})
```"#,
    )]);

    // Real local python backend + in-memory runtime
    let backend = LocalPythonBackend::default();
    let runtime = Arc::new(InMemoryComputeRuntime::new(backend, "python"));

    // Compute operator
    let op = skg_op_compute_runtime::operator::ComputeOperator::new(provider, runtime);

    // Minimal dispatch context and input
    let ctx = DispatchContext::new(DispatchId::new("example"), OperatorId::new("compute"));
    let input = OperatorInput::new(
        Content::text("Compute the answer"),
        layer0::operator::TriggerType::User,
    )
    .with_session(SessionId::new("compute-python-poc"));

    // Execute
    match op.execute(input, &ctx).await {
        Ok(output) => match output.outcome {
            Outcome::Terminal {
                terminal: TerminalOutcome::Completed,
            } => match output.message {
                Content::Text(s) => println!("stdout: {}", s),
                Content::Blocks(blocks) => {
                    for b in blocks {
                        if let layer0::content::ContentBlock::Data { data, .. } = b {
                            println!(
                                "final_result: {}",
                                serde_json::to_string_pretty(&data).unwrap()
                            );
                        }
                    }
                }
                _ => {}
            },
            other => eprintln!("unexpected outcome: {}", other),
        },
        Err(e) => {
            eprintln!("operator failed: {}", e);
            std::process::exit(1);
        }
    }
}
