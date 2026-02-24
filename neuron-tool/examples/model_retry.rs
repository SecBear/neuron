//! Example: a tool that uses `ToolError::ModelRetry` for self-correction.
//!
//! When the model provides invalid input, the tool returns a hint instead of
//! a hard error. The agent loop converts this into an error tool result so the
//! model can retry with adjusted arguments.
//!
//! Run with: `cargo run --example model_retry -p neuron-tool`

use std::collections::HashMap;
use std::path::PathBuf;

use neuron_tool::{ToolRegistry, neuron_tool};
use neuron_types::{ToolContext, ToolError};
use tokio_util::sync::CancellationToken;

#[derive(Debug, serde::Serialize)]
struct LookupResult {
    name: String,
    population: u64,
}

#[neuron_tool(
    name = "lookup_city",
    description = "Look up city information by ISO country code and city name"
)]
async fn lookup_city(
    /// ISO 3166-1 alpha-2 country code (e.g. "US", "GB", "JP")
    country_code: String,
    /// City name
    city: String,
    _ctx: &ToolContext,
) -> Result<LookupResult, ToolError> {
    // Validate the country code format — return ModelRetry so the LLM can fix it
    if country_code.len() != 2 || !country_code.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(ToolError::ModelRetry(format!(
            "country_code must be a 2-letter uppercase ISO code (e.g. \"US\"), got: \"{country_code}\""
        )));
    }

    // Simulated lookup
    Ok(LookupResult {
        name: format!("{city}, {country_code}"),
        population: 1_000_000,
    })
}

#[tokio::main]
async fn main() {
    let mut registry = ToolRegistry::new();
    registry.register(LookupCityTool);

    let ctx = ToolContext {
        cwd: PathBuf::from("/tmp"),
        session_id: "example".into(),
        environment: HashMap::new(),
        cancellation_token: CancellationToken::new(),
        progress_reporter: None,
    };

    // First call: invalid country code — would trigger ModelRetry in an agent loop
    let bad_input = serde_json::json!({
        "country_code": "united states",
        "city": "Portland"
    });

    let result = registry.execute("lookup_city", bad_input, &ctx).await;
    match result {
        Err(ToolError::ModelRetry(hint)) => {
            println!("ModelRetry hint: {hint}");
            println!("(In an agent loop, this hint becomes an error tool result");
            println!(" and the model retries with corrected arguments.)\n");
        }
        other => println!("Unexpected: {other:?}"),
    }

    // Second call: valid country code — succeeds
    let good_input = serde_json::json!({
        "country_code": "US",
        "city": "Portland"
    });

    let output = registry
        .execute("lookup_city", good_input, &ctx)
        .await
        .expect("valid call should succeed");

    println!("Success:");
    for item in &output.content {
        if let neuron_types::ContentItem::Text(text) = item {
            println!("  {text}");
        }
    }
}
