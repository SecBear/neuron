//! Built-in middleware implementations.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use neuron_types::{
    ContentItem, PermissionDecision, PermissionPolicy, ToolContext, ToolError, ToolOutput,
    WasmBoxedFuture,
};

use crate::middleware::{Next, ToolCall, ToolMiddleware};
use crate::registry::ToolRegistry;

/// Middleware that checks tool call permissions against a [`PermissionPolicy`].
///
/// If the policy returns `Deny`, the tool call is rejected with `ToolError::PermissionDenied`.
/// If the policy returns `Ask`, the tool call is rejected (external confirmation not handled here).
pub struct PermissionChecker {
    policy: Arc<dyn PermissionPolicy>,
}

impl PermissionChecker {
    /// Create a new permission checker with the given policy.
    #[must_use]
    pub fn new(policy: impl PermissionPolicy + 'static) -> Self {
        Self {
            policy: Arc::new(policy),
        }
    }
}

impl ToolMiddleware for PermissionChecker {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            match self.policy.check(&call.name, &call.input) {
                PermissionDecision::Allow => next.run(call, ctx).await,
                PermissionDecision::Deny(reason) => Err(ToolError::PermissionDenied(reason)),
                PermissionDecision::Ask(reason) => Err(ToolError::PermissionDenied(format!(
                    "requires confirmation: {reason}"
                ))),
            }
        })
    }
}

/// Middleware that truncates tool output to a maximum character length.
///
/// Long tool outputs can consume excessive tokens in the context window.
/// This middleware truncates text content items that exceed the limit.
pub struct OutputFormatter {
    max_chars: usize,
}

impl OutputFormatter {
    /// Create a new output formatter with the given character limit.
    #[must_use]
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }
}

impl ToolMiddleware for OutputFormatter {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let mut output = next.run(call, ctx).await?;

            // Truncate text content items that exceed the limit
            output.content = output
                .content
                .into_iter()
                .map(|item| match item {
                    ContentItem::Text(text) if text.len() > self.max_chars => {
                        // Find the nearest char boundary at or before max_chars
                        // to avoid slicing in the middle of a multi-byte UTF-8
                        // character. This is a stable polyfill for
                        // str::floor_char_boundary (stabilized in 1.93).
                        let mut boundary = self.max_chars;
                        while boundary > 0 && !text.is_char_boundary(boundary) {
                            boundary -= 1;
                        }
                        ContentItem::Text(format!(
                            "{}... [truncated, {} chars total]",
                            &text[..boundary],
                            text.len()
                        ))
                    }
                    other => other,
                })
                .collect();

            Ok(output)
        })
    }
}

/// Middleware that validates tool call input against the tool's JSON Schema.
///
/// Performs lightweight structural validation: checks that the input is an
/// object, required fields are present, and property types match the schema.
/// This catches obvious input errors before the tool executes, without
/// depending on a full JSON Schema validation library.
pub struct SchemaValidator {
    /// Map of tool name to its input_schema JSON value.
    schemas: HashMap<String, serde_json::Value>,
}

impl SchemaValidator {
    /// Create a new schema validator from the current tool registry.
    ///
    /// Snapshots all tool definitions at construction time. Tools registered
    /// after this call will not be validated.
    #[must_use]
    pub fn new(registry: &ToolRegistry) -> Self {
        let schemas = registry
            .definitions()
            .into_iter()
            .map(|def| (def.name, def.input_schema))
            .collect();
        Self { schemas }
    }
}

impl ToolMiddleware for SchemaValidator {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            if let Some(schema) = self.schemas.get(&call.name) {
                validate_input(&call.input, schema)?;
            }
            next.run(call, ctx).await
        })
    }
}

/// Validate a JSON input value against a JSON Schema object.
///
/// Performs lightweight structural checks:
/// - Input must be an object (if schema says `"type": "object"`)
/// - All `"required"` fields must be present
/// - Property types must match the schema's `"type"` declarations
fn validate_input(input: &serde_json::Value, schema: &serde_json::Value) -> Result<(), ToolError> {
    let schema_obj = match schema.as_object() {
        Some(obj) => obj,
        None => return Ok(()), // No schema object to validate against
    };

    // Check that the input is an object if schema declares type: "object"
    if let Some(serde_json::Value::String(ty)) = schema_obj.get("type")
        && ty == "object"
        && !input.is_object()
    {
        return Err(ToolError::InvalidInput("expected object input".to_string()));
    }

    let input_obj = match input.as_object() {
        Some(obj) => obj,
        None => return Ok(()), // Non-object input, nothing more to validate
    };

    // Check required fields
    if let Some(serde_json::Value::Array(required)) = schema_obj.get("required") {
        for field in required {
            if let Some(field_name) = field.as_str()
                && !input_obj.contains_key(field_name)
            {
                return Err(ToolError::InvalidInput(format!(
                    "missing required field: {field_name}"
                )));
            }
        }
    }

    // Check property types
    if let Some(serde_json::Value::Object(properties)) = schema_obj.get("properties") {
        for (field_name, prop_schema) in properties {
            if let Some(value) = input_obj.get(field_name)
                && let Some(serde_json::Value::String(expected_type)) = prop_schema.get("type")
                && !json_type_matches(value, expected_type)
            {
                return Err(ToolError::InvalidInput(format!(
                    "field '{field_name}' expected type '{expected_type}', \
                     got {}",
                    json_type_name(value)
                )));
            }
        }
    }

    Ok(())
}

/// Check if a JSON value matches the expected JSON Schema type string.
fn json_type_matches(value: &serde_json::Value, expected: &str) -> bool {
    match expected {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.is_i64() || value.is_u64(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true, // Unknown type, pass through
    }
}

/// Return the JSON type name for a value (for error messages).
fn json_type_name(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// Middleware that enforces a timeout on tool execution.
///
/// Wraps the downstream tool call in [`tokio::time::timeout`]. If the tool
/// does not complete within the configured duration, returns
/// `ToolError::ExecutionFailed` with a descriptive message so the model
/// can adapt.
///
/// Per-tool overrides allow different timeouts for tools with known
/// different latency profiles (e.g., web scraping vs. simple computation).
pub struct TimeoutMiddleware {
    default_timeout: Duration,
    per_tool: HashMap<String, Duration>,
}

impl TimeoutMiddleware {
    /// Create a new timeout middleware with the given default timeout.
    #[must_use]
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            default_timeout,
            per_tool: HashMap::new(),
        }
    }

    /// Set a per-tool timeout override.
    #[must_use]
    pub fn with_tool_timeout(mut self, tool_name: impl Into<String>, timeout: Duration) -> Self {
        self.per_tool.insert(tool_name.into(), timeout);
        self
    }
}

impl ToolMiddleware for TimeoutMiddleware {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let timeout = self
                .per_tool
                .get(&call.name)
                .unwrap_or(&self.default_timeout);
            match tokio::time::timeout(*timeout, next.run(call, ctx)).await {
                Ok(result) => result,
                Err(_elapsed) => Err(ToolError::ExecutionFailed(Box::new(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "tool '{}' timed out after {:.1}s",
                        call.name,
                        timeout.as_secs_f64()
                    ),
                )))),
            }
        })
    }
}

/// Middleware that validates structured output from a tool against a JSON Schema.
///
/// When attached to a "result" tool, this validates the model's JSON input
/// against the expected schema. On validation failure, returns
/// [`ToolError::ModelRetry`] with a description of what went wrong so the
/// model can self-correct.
///
/// This implements the tool-based structured output pattern used by
/// instructor, Pydantic AI, and Rig: inject a tool with the output schema,
/// force the model to call it, and validate.
pub struct StructuredOutputValidator {
    schema: serde_json::Value,
    max_retries: usize,
}

impl StructuredOutputValidator {
    /// Create a new structured output validator.
    ///
    /// The `schema` should be a JSON Schema object describing the expected
    /// output shape. `max_retries` limits how many times the model can
    /// retry on validation failure (0 means fail immediately on first error).
    #[must_use]
    pub fn new(schema: serde_json::Value, max_retries: usize) -> Self {
        Self {
            schema,
            max_retries,
        }
    }
}

impl ToolMiddleware for StructuredOutputValidator {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            // Validate the input (which IS the structured output from the model)
            // against the schema before passing to the tool
            if let Err(e) = validate_input(&call.input, &self.schema) {
                // Return ModelRetry so the model can self-correct
                return Err(ToolError::ModelRetry(format!(
                    "Output validation failed: {e}. Please fix the output to match the schema."
                )));
            }
            next.run(call, ctx).await
        })
    }
}

/// Tracks retry count for structured output validation.
///
/// Wraps [`StructuredOutputValidator`] and enforces a maximum number of
/// retries. After `max_retries` validation failures, converts the error
/// to `ToolError::InvalidInput` (non-retryable).
pub struct RetryLimitedValidator {
    inner: StructuredOutputValidator,
    attempts: std::sync::atomic::AtomicUsize,
}

impl RetryLimitedValidator {
    /// Create a new retry-limited validator wrapping a [`StructuredOutputValidator`].
    #[must_use]
    pub fn new(validator: StructuredOutputValidator) -> Self {
        Self {
            inner: validator,
            attempts: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl ToolMiddleware for RetryLimitedValidator {
    fn process<'a>(
        &'a self,
        call: &'a ToolCall,
        ctx: &'a ToolContext,
        next: Next<'a>,
    ) -> WasmBoxedFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            if let Err(e) = validate_input(&call.input, &self.inner.schema) {
                let attempt = self
                    .attempts
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if attempt >= self.inner.max_retries {
                    return Err(ToolError::InvalidInput(format!(
                        "Output validation failed after {} retries: {e}",
                        self.inner.max_retries
                    )));
                }
                return Err(ToolError::ModelRetry(format!(
                    "Output validation failed (attempt {}/{}): {e}. \
                     Please fix the output to match the schema.",
                    attempt + 1,
                    self.inner.max_retries
                )));
            }
            // Reset attempt counter on success
            self.attempts.store(0, std::sync::atomic::Ordering::Relaxed);
            next.run(call, ctx).await
        })
    }
}
