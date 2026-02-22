//! Basic usage of the Anthropic provider.
//!
//! Set ANTHROPIC_API_KEY in your environment and run:
//!   cargo run --example basic

use neuron_provider_anthropic::Anthropic;
use neuron_types::{CompletionRequest, ContentBlock, Message, Provider, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY environment variable must be set");

    let provider = Anthropic::new(api_key);

    let request = CompletionRequest {
        model: String::new(), // use default
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Say hello in one sentence.".into())],
        }],
        system: None,
        tools: vec![],
        max_tokens: Some(128),
        temperature: None,
        top_p: None,
        stop_sequences: vec![],
        tool_choice: None,
        response_format: None,
        thinking: None,
        reasoning_effort: None,
        extra: None,
    };

    let response = provider.complete(request).await?;
    println!("Response: {:?}", response.message.content);
    println!("Tokens: {} in / {} out", response.usage.input_tokens, response.usage.output_tokens);

    Ok(())
}
