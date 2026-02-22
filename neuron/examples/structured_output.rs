//! Structured output: JSON Schema-constrained responses.
//!
//! Set ANTHROPIC_API_KEY in your environment and run:
//!   cargo run --example structured_output -p neuron

use neuron::anthropic::Anthropic;
use neuron::prelude::*;

/// A movie review with structured fields.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct MovieReview {
    /// Title of the movie.
    title: String,
    /// Rating from 1 to 10.
    rating: u8,
    /// Brief summary of the review.
    summary: String,
    /// Whether the reviewer recommends the movie.
    recommended: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create an Anthropic provider from the environment variable.
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY environment variable must be set");
    let provider = Anthropic::new(api_key);

    // 2. Generate JSON Schema from the Rust struct using schemars.
    let schema = schemars::schema_for!(MovieReview);
    let schema_value = serde_json::to_value(schema)?;

    // 3. Build a CompletionRequest with JSON Schema response format.
    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text(
                "Review the movie Inception in JSON format.".to_string(),
            )],
        }],
        max_tokens: Some(1024),
        response_format: Some(neuron_types::ResponseFormat::JsonSchema {
            name: "movie_review".into(),
            schema: schema_value,
            strict: true,
        }),
        ..Default::default()
    };

    // 4. Send the request.
    let response = provider.complete(request).await?;

    // 5. Extract the text from the response.
    let text = response
        .message
        .content
        .iter()
        .find_map(|block| match block {
            ContentBlock::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .expect("response should contain text");

    println!("Raw JSON response:\n{text}\n");

    // 6. Parse the JSON into our typed struct.
    let review: MovieReview = serde_json::from_str(text)?;

    println!("Parsed MovieReview:");
    println!("  Title:       {}", review.title);
    println!("  Rating:      {}/10", review.rating);
    println!("  Summary:     {}", review.summary);
    println!("  Recommended: {}", review.recommended);

    Ok(())
}
