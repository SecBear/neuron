//! Proc-macro crate for deriving Tool implementations.
//!
//! Provides the `#[agent_tool]` attribute macro.

use proc_macro::TokenStream;

/// Derive a Tool implementation from an async function.
///
/// # Example
///
/// ```ignore
/// #[agent_tool(name = "calculate", description = "Evaluate a math expression")]
/// async fn calculate(expression: String, _ctx: &ToolContext) -> Result<Output, Error> {
///     todo!()
/// }
/// ```
#[proc_macro_attribute]
pub fn agent_tool(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Stub — will be implemented in Task 2.4
    item
}
