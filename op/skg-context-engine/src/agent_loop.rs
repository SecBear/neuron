//! Agentic ReAct loop implementing the [`Operator`] protocol.
//!
//! [`AgentLoop`] drives the infer → act → observe cycle. It wires together:
//!
//! - A [`Provider`] for LLM inference,
//! - An [`AgentBehaviour`] for developer-defined loop control,
//! - A router [`Operator`] for dispatching tool/sub-agent calls,
//! - A [`ReactivePipeline`] factory for per-invocation middleware.
//!
//! `handle()` returns immediately with a [`DispatchHandle`]. The loop runs in
//! a spawned tokio task and streams [`DispatchEvent`]s through the channel.

use std::sync::Arc;

use async_trait::async_trait;
use layer0::capability::CapabilityDescriptor;
use layer0::content::Content;
use layer0::dispatch::{DispatchEvent, DispatchHandle, DispatchSender};
use layer0::dispatch_context::DispatchContext;
use layer0::error::ProtocolError;
use layer0::id::{DispatchId, OperatorId};
use layer0::operator::{
    InterceptionKind, Operator, OperatorInput, OperatorOutput, Outcome, TransferOutcome,
    TriggerType,
};
use skg_turn::provider::Provider;

use crate::agent_behaviour::{AgentBehaviour, LoopDecision};
use crate::agent_event::AgentEvent;
use crate::compile::CompileConfig;
use crate::context::Context;
use crate::context_op::OpResult;
use crate::reactive_pipeline::ReactivePipeline;

/// Factory closure that produces a fresh [`ReactivePipeline`] per invocation.
///
/// Stored as an `Arc` so it can be shared across clones of the loop and moved
/// into spawned tasks cheaply.
pub type PipelineFactory = Arc<dyn Fn() -> ReactivePipeline + Send + Sync>;

/// Agentic ReAct loop implementing [`Operator`].
///
/// Drives the infer → act → observe cycle, delegating lifecycle decisions to
/// an [`AgentBehaviour`] and tool dispatch to a router [`Operator`].
///
/// # Concurrency
///
/// Each invocation of [`handle`](Operator::handle) spawns an independent tokio
/// task. Multiple concurrent calls produce independent loops. The `provider`,
/// `behaviour`, `router`, and `pipeline_factory` are shared via `Arc` across
/// all concurrent invocations.
pub struct AgentLoop<P: Provider, B: AgentBehaviour> {
    provider: Arc<P>,
    behaviour: Arc<B>,
    router: Arc<dyn Operator>,
    pipeline_factory: PipelineFactory,
    descriptor: CapabilityDescriptor,
}

impl<P: Provider, B: AgentBehaviour> AgentLoop<P, B> {
    /// Create a new [`AgentLoop`].
    ///
    /// - `descriptor` — capability metadata returned by [`Operator::descriptor`].
    /// - `provider` — LLM backend for inference.
    /// - `behaviour` — developer callbacks controlling loop decisions.
    /// - `router` — operator that routes tool/sub-agent dispatch calls.
    /// - `pipeline_factory` — closure producing a fresh [`ReactivePipeline`]
    ///   per invocation.
    pub fn new(
        descriptor: CapabilityDescriptor,
        provider: P,
        behaviour: B,
        router: Arc<dyn Operator>,
        pipeline_factory: PipelineFactory,
    ) -> Self {
        Self {
            provider: Arc::new(provider),
            behaviour: Arc::new(behaviour),
            router,
            pipeline_factory,
            descriptor,
        }
    }
}

#[async_trait]
impl<P: Provider + 'static, B: AgentBehaviour + 'static> Operator for AgentLoop<P, B> {
    fn descriptor(&self) -> CapabilityDescriptor {
        self.descriptor.clone()
    }

    async fn handle(
        &self,
        input: OperatorInput,
        ctx: &DispatchContext,
    ) -> Result<DispatchHandle, ProtocolError> {
        let dispatch_id = ctx.dispatch_id.clone();
        let (handle, sender) = DispatchHandle::channel(dispatch_id);

        let provider = Arc::clone(&self.provider);
        let behaviour = Arc::clone(&self.behaviour);
        let router = Arc::clone(&self.router);
        let pipeline_factory = Arc::clone(&self.pipeline_factory);
        let dispatch_ctx = ctx.clone();

        tokio::spawn(async move {
            run_loop(
                provider,
                behaviour,
                router,
                pipeline_factory,
                input,
                dispatch_ctx,
                sender,
            )
            .await;
        });

        Ok(handle)
    }
}

// ── Inner loop ────────────────────────────────────────────────────────────────

/// Run the agent loop to completion inside a spawned task.
///
/// All terminal events are sent through `sender`. Returning from this function
/// ends the dispatch.
async fn run_loop<P, B>(
    provider: Arc<P>,
    behaviour: Arc<B>,
    router: Arc<dyn Operator>,
    pipeline_factory: PipelineFactory,
    input: OperatorInput,
    dispatch_ctx: DispatchContext,
    sender: DispatchSender,
) where
    P: Provider,
    B: AgentBehaviour,
{
    let pipeline = (pipeline_factory)();
    let mut ctx = behaviour.init_context(&input, &dispatch_ctx).await;

    // ── LoopStarted ───────────────────────────────────────────────────────────
    if let Some(outcome) =
        pipeline_halt(&pipeline.emit(&AgentEvent::LoopStarted, &mut ctx, &dispatch_ctx).await)
    {
        send_ending(&pipeline, &mut ctx, &dispatch_ctx, outcome, &sender).await;
        return;
    }

    // ── Main infer/act loop ───────────────────────────────────────────────────
    loop {
        // a. BeforeInference ──────────────────────────────────────────────────
        if let Some(outcome) = pipeline_halt(
            &pipeline
                .emit(&AgentEvent::BeforeInference, &mut ctx, &dispatch_ctx)
                .await,
        ) {
            send_ending(&pipeline, &mut ctx, &dispatch_ctx, outcome, &sender).await;
            return;
        }

        // b/c. Capabilities + compile ─────────────────────────────────────────
        // Descriptors available for future tool-schema compilation.
        let _caps = behaviour.capabilities(&ctx);
        let compiled = ctx.compile(&CompileConfig::default());

        // d. Infer ────────────────────────────────────────────────────────────
        let response = match compiled.infer(&*provider).await {
            Ok(r) => r.response,
            Err(e) => {
                let error = ProtocolError::internal(format!("inference failed: {e}"));
                let _ = sender.send(DispatchEvent::Failed { error }).await;
                return;
            }
        };

        // e. AfterInference ───────────────────────────────────────────────────
        if let Some(outcome) = pipeline_halt(
            &pipeline
                .emit(
                    &AgentEvent::AfterInference {
                        response: response.clone(),
                    },
                    &mut ctx,
                    &dispatch_ctx,
                )
                .await,
        ) {
            send_ending(&pipeline, &mut ctx, &dispatch_ctx, outcome, &sender).await;
            return;
        }

        // f. Behaviour: handle_response ───────────────────────────────────────
        match behaviour.handle_response(&response, &mut ctx).await {
            LoopDecision::Complete(output) => {
                let outcome = output.outcome.clone();
                let _ = pipeline
                    .emit(
                        &AgentEvent::LoopEnding { outcome },
                        &mut ctx,
                        &dispatch_ctx,
                    )
                    .await;
                let _ = sender.send(DispatchEvent::Completed { output }).await;
                return;
            }
            LoopDecision::Suspend(reason) => {
                let outcome = Outcome::Suspended { reason };
                let _ = pipeline
                    .emit(
                        &AgentEvent::LoopEnding {
                            outcome: outcome.clone(),
                        },
                        &mut ctx,
                        &dispatch_ctx,
                    )
                    .await;
                let output = OperatorOutput::new(Content::text(""), outcome);
                let _ = sender.send(DispatchEvent::Completed { output }).await;
                return;
            }
            LoopDecision::Delegate(target_id, _op_input) => {
                let outcome = Outcome::Transfer {
                    transfer: TransferOutcome::Delegated,
                };
                let _ = pipeline
                    .emit(
                        &AgentEvent::LoopEnding {
                            outcome: outcome.clone(),
                        },
                        &mut ctx,
                        &dispatch_ctx,
                    )
                    .await;
                let output = OperatorOutput::new(
                    Content::text(format!("delegated to {target_id}")),
                    outcome,
                );
                let _ = sender.send(DispatchEvent::Completed { output }).await;
                return;
            }
            LoopDecision::Continue => {
                // g/h. Dispatch tool calls sequentially if present ─────────────
                if dispatch_tool_calls(
                    &response,
                    &behaviour,
                    &router,
                    &pipeline,
                    &mut ctx,
                    &dispatch_ctx,
                    &sender,
                )
                .await
                {
                    // dispatch_tool_calls returns true when it already sent
                    // a terminal event (Complete/Suspend/Delegate from behaviour).
                    return;
                }
                // Continue to next inference iteration.
            }
        }
    }
}

/// Dispatch all tool calls in `response` to the router, sequentially.
///
/// Returns `true` if the loop should terminate (a terminal event has already
/// been sent through `sender`), `false` to continue iterating.
async fn dispatch_tool_calls<B: AgentBehaviour>(
    response: &skg_turn::infer::InferResponse,
    behaviour: &Arc<B>,
    router: &Arc<dyn Operator>,
    pipeline: &ReactivePipeline,
    ctx: &mut Context,
    dispatch_ctx: &DispatchContext,
    sender: &DispatchSender,
) -> bool {
    if !response.has_tool_calls() {
        return false;
    }

    for call in &response.tool_calls {
        let action_id = OperatorId::new(&call.name);
        // Derive a unique dispatch ID from the parent + call ID.
        let child_dispatch_id =
            DispatchId::new(format!("{}-{}", dispatch_ctx.dispatch_id, call.id));
        let child_ctx = dispatch_ctx.child(child_dispatch_id, action_id.clone());

        let action_input = OperatorInput::new(
            Content::text(serde_json::to_string(&call.input).unwrap_or_default()),
            TriggerType::Task,
        );

        let _ = pipeline
            .emit(
                &AgentEvent::ActionRequested {
                    id: action_id.clone(),
                    input: action_input.clone(),
                },
                ctx,
                dispatch_ctx,
            )
            .await;

        let action_result = match router.handle(action_input, &child_ctx).await {
            Ok(h) => h.collect().await,
            Err(e) => Err(e),
        };

        match action_result {
            Ok(action_output) => {
                let _ = pipeline
                    .emit(
                        &AgentEvent::ActionCompleted {
                            id: action_id.clone(),
                            output: action_output.clone(),
                        },
                        ctx,
                        dispatch_ctx,
                    )
                    .await;

                let decision = behaviour
                    .handle_action_result(&action_id, &action_output, ctx)
                    .await;

                if let Some(terminated) =
                    apply_decision(decision, pipeline, ctx, dispatch_ctx, sender).await
                {
                    return terminated;
                }
                // Continue means go to the next tool call.
            }
            Err(error) => {
                // Emit ActionFailed; no behaviour hook for error path per spec.
                tracing::warn!(
                    action = %action_id,
                    error = %error,
                    "action failed"
                );
                let _ = pipeline
                    .emit(
                        &AgentEvent::ActionFailed {
                            id: action_id.clone(),
                            error,
                        },
                        ctx,
                        dispatch_ctx,
                    )
                    .await;
            }
        }
    }

    false
}

/// Apply a [`LoopDecision`], sending terminal events and returning whether the
/// loop should terminate.
///
/// Returns `Some(true)` when a terminal event was sent, `None` on `Continue`.
async fn apply_decision(
    decision: LoopDecision,
    pipeline: &ReactivePipeline,
    ctx: &mut Context,
    dispatch_ctx: &DispatchContext,
    sender: &DispatchSender,
) -> Option<bool> {
    match decision {
        LoopDecision::Continue => None,
        LoopDecision::Complete(output) => {
            let outcome = output.outcome.clone();
            let _ = pipeline
                .emit(
                    &AgentEvent::LoopEnding { outcome },
                    ctx,
                    dispatch_ctx,
                )
                .await;
            let _ = sender.send(DispatchEvent::Completed { output }).await;
            Some(true)
        }
        LoopDecision::Suspend(reason) => {
            let outcome = Outcome::Suspended { reason };
            let _ = pipeline
                .emit(
                    &AgentEvent::LoopEnding {
                        outcome: outcome.clone(),
                    },
                    ctx,
                    dispatch_ctx,
                )
                .await;
            let output = OperatorOutput::new(Content::text(""), outcome);
            let _ = sender.send(DispatchEvent::Completed { output }).await;
            Some(true)
        }
        LoopDecision::Delegate(target_id, _op_input) => {
            let outcome = Outcome::Transfer {
                transfer: TransferOutcome::Delegated,
            };
            let _ = pipeline
                .emit(
                    &AgentEvent::LoopEnding {
                        outcome: outcome.clone(),
                    },
                    ctx,
                    dispatch_ctx,
                )
                .await;
            let output = OperatorOutput::new(
                Content::text(format!("delegated to {target_id}")),
                outcome,
            );
            let _ = sender.send(DispatchEvent::Completed { output }).await;
            Some(true)
        }
    }
}

// ── Small helpers ─────────────────────────────────────────────────────────────

/// Convert a pipeline [`OpResult`] to an `Outcome` if the loop must stop.
///
/// `Halt` maps to `Intercepted`; `Suspend` maps to `Suspended`.
/// `Continue` and `Skip` return `None`.
fn pipeline_halt(result: &OpResult) -> Option<Outcome> {
    match result {
        OpResult::Halt(reason) => Some(Outcome::Intercepted {
            interception: InterceptionKind::PolicyHalt {
                reason: reason.clone(),
            },
        }),
        OpResult::Suspend(reason) => Some(Outcome::Suspended {
            reason: reason.clone(),
        }),
        OpResult::Continue | OpResult::Skip => None,
    }
}

/// Emit `LoopEnding` through the pipeline and send a `Completed` dispatch event.
async fn send_ending(
    pipeline: &ReactivePipeline,
    ctx: &mut Context,
    dispatch_ctx: &DispatchContext,
    outcome: Outcome,
    sender: &DispatchSender,
) {
    let _ = pipeline
        .emit(
            &AgentEvent::LoopEnding {
                outcome: outcome.clone(),
            },
            ctx,
            dispatch_ctx,
        )
        .await;
    let output = OperatorOutput::new(Content::text(""), outcome);
    let _ = sender.send(DispatchEvent::Completed { output }).await;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_behaviour::{AgentBehaviour, LoopDecision};
    use crate::agent_event::EventKind;
    use crate::context::Context;
    use crate::context_op::{OpResult, on};
    use crate::reactive_pipeline::ReactivePipeline;
    use async_trait::async_trait;
    use layer0::capability::{
        ApprovalFacts, AuthFacts, CapabilityDescriptor, CapabilityId, CapabilityKind,
        ExecutionClass, SchedulingFacts,
    };
    use layer0::content::Content;
    use layer0::dispatch_context::DispatchContext;
    use layer0::id::{DispatchId, OperatorId};
    use layer0::operator::{
        Operator, OperatorInput, OperatorOutput, Outcome, TerminalOutcome, TriggerType,
        completed_handle,
    };
    use layer0::ProtocolError;
    use skg_turn::infer::InferResponse;
    use skg_turn::test_utils::{TestProvider, make_text_response, make_tool_call_response};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    fn test_descriptor() -> CapabilityDescriptor {
        CapabilityDescriptor::new(
            CapabilityId::new("test.agent_loop"),
            CapabilityKind::Agent,
            "TestAgentLoop",
            "Agent loop under test",
            SchedulingFacts::new(ExecutionClass::Shared, false, false, true, None),
            ApprovalFacts::None,
            AuthFacts::Open,
        )
    }

    fn test_dispatch_ctx() -> DispatchContext {
        DispatchContext::new(DispatchId::new("test-dispatch"), OperatorId::new("test-op"))
    }

    fn completed_output() -> OperatorOutput {
        OperatorOutput::new(
            Content::text("done"),
            Outcome::Terminal {
                terminal: TerminalOutcome::Completed,
            },
        )
    }

    // ── Test 1: Complete on first response ────────────────────────────────────

    struct CompleteBehaviour;

    #[async_trait]
    impl AgentBehaviour for CompleteBehaviour {
        async fn init_context(
            &self,
            _input: &OperatorInput,
            _ctx: &DispatchContext,
        ) -> Context {
            Context::new()
        }

        fn capabilities(&self, _ctx: &Context) -> Vec<CapabilityDescriptor> {
            vec![]
        }

        async fn handle_response(
            &self,
            _response: &InferResponse,
            _ctx: &mut Context,
        ) -> LoopDecision {
            LoopDecision::Complete(completed_output())
        }

        async fn handle_action_result(
            &self,
            _action: &OperatorId,
            _result: &OperatorOutput,
            _ctx: &mut Context,
        ) -> LoopDecision {
            LoopDecision::Continue
        }
    }

    #[tokio::test]
    async fn complete_on_first_response_emits_completed_event() {
        let provider = TestProvider::with_responses(vec![make_text_response("hello")]);
        let loop_op = AgentLoop::new(
            test_descriptor(),
            provider,
            CompleteBehaviour,
            Arc::new(layer0::test_utils::EchoOperator),
            Arc::new(ReactivePipeline::new),
        );

        let input = OperatorInput::new(Content::text("go"), TriggerType::User);
        let ctx = test_dispatch_ctx();
        let handle = loop_op.handle(input, &ctx).await.expect("handle");
        let output = handle.collect().await.expect("collect");

        assert!(
            matches!(
                output.outcome,
                Outcome::Terminal {
                    terminal: TerminalOutcome::Completed
                }
            ),
            "expected Terminal::Completed, got {:?}",
            output.outcome
        );
        assert_eq!(output.message.as_text(), Some("done"));
    }

    #[tokio::test]
    async fn agent_loop_is_storable_as_arc_dyn_operator() {
        let provider = TestProvider::with_responses(vec![make_text_response("hi")]);
        let loop_op: Arc<dyn Operator> = Arc::new(AgentLoop::new(
            test_descriptor(),
            provider,
            CompleteBehaviour,
            Arc::new(layer0::test_utils::EchoOperator),
            Arc::new(ReactivePipeline::new),
        ));

        let input = OperatorInput::new(Content::text("go"), TriggerType::User);
        let ctx = test_dispatch_ctx();
        let handle = loop_op.handle(input, &ctx).await.expect("handle");
        let output = handle.collect().await.expect("collect");
        assert!(matches!(
            output.outcome,
            Outcome::Terminal {
                terminal: TerminalOutcome::Completed
            }
        ));
    }

    // ── Test 2: Tool calls dispatch to router ─────────────────────────────────

    /// Behaviour that returns Complete after the first action completes.
    struct ToolBehaviour {
        action_completed: Arc<AtomicBool>,
    }

    #[async_trait]
    impl AgentBehaviour for ToolBehaviour {
        async fn init_context(
            &self,
            _input: &OperatorInput,
            _ctx: &DispatchContext,
        ) -> Context {
            Context::new()
        }

        fn capabilities(&self, _ctx: &Context) -> Vec<CapabilityDescriptor> {
            vec![]
        }

        async fn handle_response(
            &self,
            _response: &InferResponse,
            _ctx: &mut Context,
        ) -> LoopDecision {
            // After action completes, end the loop on the next inference.
            if self.action_completed.load(Ordering::SeqCst) {
                return LoopDecision::Complete(completed_output());
            }
            LoopDecision::Continue
        }

        async fn handle_action_result(
            &self,
            _action: &OperatorId,
            _result: &OperatorOutput,
            _ctx: &mut Context,
        ) -> LoopDecision {
            self.action_completed.store(true, Ordering::SeqCst);
            LoopDecision::Continue
        }
    }

    /// Router that counts calls and echoes input back.
    struct RecordingRouter {
        call_count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl Operator for RecordingRouter {
        fn descriptor(&self) -> CapabilityDescriptor {
            test_descriptor()
        }

        async fn handle(
            &self,
            input: OperatorInput,
            ctx: &DispatchContext,
        ) -> Result<layer0::dispatch::DispatchHandle, ProtocolError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let output = OperatorOutput::new(
                input.message,
                Outcome::Terminal {
                    terminal: TerminalOutcome::Completed,
                },
            );
            Ok(completed_handle(ctx.dispatch_id.clone(), output))
        }
    }

    #[tokio::test]
    async fn tool_calls_dispatched_to_router() {
        // First infer: tool call. Second infer (after action): text → Complete.
        let provider = TestProvider::with_responses(vec![
            make_tool_call_response("my_tool", "call-1", serde_json::json!({"key": "value"})),
            make_text_response("final"),
        ]);

        let router_calls = Arc::new(AtomicU32::new(0));
        let router = RecordingRouter {
            call_count: Arc::clone(&router_calls),
        };

        let loop_op = AgentLoop::new(
            test_descriptor(),
            provider,
            ToolBehaviour {
                action_completed: Arc::new(AtomicBool::new(false)),
            },
            Arc::new(router),
            Arc::new(ReactivePipeline::new),
        );

        let input = OperatorInput::new(Content::text("use a tool"), TriggerType::User);
        let ctx = test_dispatch_ctx();
        let handle = loop_op.handle(input, &ctx).await.expect("handle");
        let output = handle.collect().await.expect("collect");

        assert_eq!(router_calls.load(Ordering::SeqCst), 1, "router called once");
        assert!(
            matches!(
                output.outcome,
                Outcome::Terminal {
                    terminal: TerminalOutcome::Completed
                }
            ),
            "expected Terminal::Completed, got {:?}",
            output.outcome
        );
    }

    // ── Test 3: Pipeline events fire in correct order ─────────────────────────

    #[tokio::test]
    async fn pipeline_events_fire_in_order() {
        let events: Arc<Mutex<Vec<EventKind>>> = Arc::new(Mutex::new(Vec::new()));

        // Build a factory that captures the shared events vec and creates a
        // fresh pipeline on each call.
        let factory: PipelineFactory = {
            let events = Arc::clone(&events);
            Arc::new(move || {
                let e1 = Arc::clone(&events);
                let e2 = Arc::clone(&events);
                let e3 = Arc::clone(&events);
                let e4 = Arc::clone(&events);
                ReactivePipeline::new()
                    .add(on(EventKind::LoopStarted).apply(move |_, _, _| {
                        e1.lock().unwrap().push(EventKind::LoopStarted);
                        async { OpResult::Continue }
                    }))
                    .add(on(EventKind::BeforeInference).apply(move |_, _, _| {
                        e2.lock().unwrap().push(EventKind::BeforeInference);
                        async { OpResult::Continue }
                    }))
                    .add(on(EventKind::AfterInference).apply(move |_, _, _| {
                        e3.lock().unwrap().push(EventKind::AfterInference);
                        async { OpResult::Continue }
                    }))
                    .add(on(EventKind::LoopEnding).apply(move |_, _, _| {
                        e4.lock().unwrap().push(EventKind::LoopEnding);
                        async { OpResult::Continue }
                    }))
            })
        };

        let provider = TestProvider::with_responses(vec![make_text_response("response")]);
        let loop_op = AgentLoop::new(
            test_descriptor(),
            provider,
            CompleteBehaviour,
            Arc::new(layer0::test_utils::EchoOperator),
            factory,
        );

        let input = OperatorInput::new(Content::text("go"), TriggerType::User);
        let ctx = test_dispatch_ctx();
        let handle = loop_op.handle(input, &ctx).await.expect("handle");
        handle.collect().await.expect("collect");

        let recorded = events.lock().unwrap().clone();
        assert_eq!(
            recorded,
            vec![
                EventKind::LoopStarted,
                EventKind::BeforeInference,
                EventKind::AfterInference,
                EventKind::LoopEnding,
            ],
            "events must fire in lifecycle order"
        );
    }
}
