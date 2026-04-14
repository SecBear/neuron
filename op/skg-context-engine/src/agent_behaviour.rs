//! Agent behaviour trait — the developer extension point for agentic loops.
//!
//! [`AgentBehaviour`] defines four callbacks invoked by [`AgentLoop`](crate::AgentLoop)
//! at each lifecycle point: context initialisation, capability selection,
//! response handling, and action-result handling. Each callback returns a
//! [`LoopDecision`] that controls the loop's next step.

use async_trait::async_trait;
use layer0::capability::CapabilityDescriptor;
use layer0::dispatch_context::DispatchContext;
use layer0::id::OperatorId;
use layer0::operator::{OperatorInput, OperatorOutput};
use layer0::wait::WaitReason;
use skg_turn::infer::InferResponse;

use crate::context::Context;

/// Decision returned by behaviour callbacks to control the loop.
///
/// Returned by [`AgentBehaviour::handle_response`] and
/// [`AgentBehaviour::handle_action_result`] to tell the loop what to do next.
#[non_exhaustive]
#[derive(Debug)]
pub enum LoopDecision {
    /// Continue to the next iteration (or the next action in the current turn).
    Continue,
    /// Complete the loop successfully and emit the given output.
    Complete(OperatorOutput),
    /// Suspend the loop, waiting for the given external condition.
    Suspend(WaitReason),
    /// Delegate control to another operator and end this loop.
    ///
    /// The first field is the target operator. The second is the input to send.
    Delegate(OperatorId, OperatorInput),
}

/// Callbacks for an agentic loop. The developer's primary extension point.
///
/// Implement this trait to control how the loop:
/// - initialises context from incoming input,
/// - selects capabilities (tools) each turn,
/// - reacts to model responses,
/// - reacts to completed or failed actions.
///
/// # Example
///
/// ```ignore
/// struct SimpleAgent;
///
/// #[async_trait]
/// impl AgentBehaviour for SimpleAgent {
///     async fn init_context(&self, input: &OperatorInput, _ctx: &DispatchContext) -> Context {
///         let mut ctx = Context::new();
///         ctx.push_user(input.message.as_text().unwrap_or_default());
///         ctx
///     }
///
///     fn capabilities(&self, _ctx: &Context) -> Vec<CapabilityDescriptor> {
///         vec![]
///     }
///
///     async fn handle_response(&self, _response: &InferResponse, _ctx: &mut Context) -> LoopDecision {
///         let output = OperatorOutput::new(
///             Content::text("done"),
///             Outcome::Terminal { terminal: TerminalOutcome::Completed },
///         );
///         LoopDecision::Complete(output)
///     }
///
///     async fn handle_action_result(&self, _action: &OperatorId, _result: &OperatorOutput, _ctx: &mut Context) -> LoopDecision {
///         LoopDecision::Continue
///     }
/// }
/// ```
#[async_trait]
pub trait AgentBehaviour: Send + Sync {
    /// Build the initial [`Context`] from the operator input.
    ///
    /// Called once when the loop starts, before the first inference. Use this
    /// to inject system prompts, seed conversation history, and attach any
    /// initial context needed for the turn.
    async fn init_context(&self, input: &OperatorInput, ctx: &DispatchContext) -> Context;

    /// Return the capability descriptors available for this turn.
    ///
    /// Called once per iteration, before compiling the context for inference.
    /// The returned descriptors advertise which tools the model may call. For
    /// now the loop compiles without tool schemas; a future enhancement will
    /// convert descriptors into [`skg_turn::types::ToolSchema`] entries.
    fn capabilities(&self, ctx: &Context) -> Vec<CapabilityDescriptor>;

    /// Decide what to do after the model responds.
    ///
    /// Called with the full [`InferResponse`] after the post-inference pipeline
    /// phase completes. The behaviour can inspect the response, mutate `ctx`,
    /// and return a [`LoopDecision`] controlling the next step.
    ///
    /// Returning [`LoopDecision::Continue`] causes the loop to process any
    /// tool calls in the response and then start a new iteration.
    async fn handle_response(&self, response: &InferResponse, ctx: &mut Context) -> LoopDecision;

    /// Decide what to do after an action (tool/sub-agent) completes.
    ///
    /// Called for each action that completes successfully. The behaviour can
    /// inject tool-result messages into `ctx` and return a [`LoopDecision`]
    /// to continue, complete, suspend, or delegate.
    async fn handle_action_result(
        &self,
        action: &OperatorId,
        result: &OperatorOutput,
        ctx: &mut Context,
    ) -> LoopDecision;
}
