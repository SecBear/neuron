//! Multi-provider: same request sent to Anthropic and OpenAI.
//!
//! Demonstrates provider-agnostic design — the same CompletionRequest works
//! with any Provider implementation.
//!
//! Set ANTHROPIC_API_KEY and OPENAI_API_KEY, then run:
//!   cargo run --example multi_provider -p neuron --features openai

use neuron::anthropic::Anthropic;
use neuron::openai::OpenAi;
use neuron::prelude::*;

/// Send a request to any provider and print the result.
async fn ask(name: &str, provider: &impl Provider, request: CompletionRequest) {
    println!("--- {name} ---");
    match provider.complete(request).await {
        Ok(response) => {
            let text = response
                .message
                .content
                .iter()
                .find_map(|block| match block {
                    ContentBlock::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .unwrap_or("[no text in response]");

            println!("Model:   {}", response.model);
            println!("Response: {text}");
            println!(
                "Usage:   {} input / {} output tokens",
                response.usage.input_tokens, response.usage.output_tokens
            );
        }
        Err(e) => {
            eprintln!("Error from {name}: {e}");
        }
    }
    println!();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Create providers from environment variables.
    let anthropic_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY environment variable must be set");
    let openai_key = std::env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY environment variable must be set");

    let anthropic = Anthropic::new(anthropic_key);
    let openai = OpenAi::new(openai_key);

    // 2. Build a single request — clone it for each provider.
    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text(
                "In one sentence, what is the theory of relativity?".to_string(),
            )],
        }],
        max_tokens: Some(256),
        ..Default::default()
    };

    // 3. Send to both providers.
    ask("Anthropic", &anthropic, request.clone()).await;
    ask("OpenAI", &openai, request).await;

    Ok(())
}
