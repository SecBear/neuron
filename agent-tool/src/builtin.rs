//! Built-in middleware implementations.

use std::collections::HashMap;
use std::sync::Arc;

use agent_types::{
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
                PermissionDecision::Deny(reason) => {
                    Err(ToolError::PermissionDenied(reason))
                }
                PermissionDecision::Ask(reason) => {
                    Err(ToolError::PermissionDenied(format!(
                        "requires confirmation: {reason}"
                    )))
                }
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
                        // Use floor_char_boundary to avoid slicing in the
                        // middle of a multi-byte UTF-8 character.
                        let boundary = text.floor_char_boundary(self.max_chars);
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
fn validate_input(
    input: &serde_json::Value,
    schema: &serde_json::Value,
) -> Result<(), ToolError> {
    let schema_obj = match schema.as_object() {
        Some(obj) => obj,
        None => return Ok(()), // No schema object to validate against
    };

    // Check that the input is an object if schema declares type: "object"
    if let Some(serde_json::Value::String(ty)) = schema_obj.get("type") {
        if ty == "object" {
            if !input.is_object() {
                return Err(ToolError::InvalidInput(
                    "expected object input".to_string(),
                ));
            }
        }
    }

    let input_obj = match input.as_object() {
        Some(obj) => obj,
        None => return Ok(()), // Non-object input, nothing more to validate
    };

    // Check required fields
    if let Some(serde_json::Value::Array(required)) = schema_obj.get("required") {
        for field in required {
            if let Some(field_name) = field.as_str() {
                if !input_obj.contains_key(field_name) {
                    return Err(ToolError::InvalidInput(format!(
                        "missing required field: {field_name}"
                    )));
                }
            }
        }
    }

    // Check property types
    if let Some(serde_json::Value::Object(properties)) = schema_obj.get("properties") {
        for (field_name, prop_schema) in properties {
            if let Some(value) = input_obj.get(field_name) {
                if let Some(serde_json::Value::String(expected_type)) =
                    prop_schema.get("type")
                {
                    if !json_type_matches(value, expected_type) {
                        return Err(ToolError::InvalidInput(format!(
                            "field '{field_name}' expected type '{expected_type}', \
                             got {}",
                            json_type_name(value)
                        )));
                    }
                }
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
