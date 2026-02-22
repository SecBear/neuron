//! Basic usage of the Ollama provider.
//!
//! Make sure Ollama is running locally and run:
//!   cargo run --example basic

use neuron_provider_ollama::Ollama;
use neuron_types::{CompletionRequest, ContentBlock, Message, Provider, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Ollama::new();

    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text("Say hello in one sentence.".into())],
        }],
        max_tokens: Some(128),
        ..Default::default()
    };

    let response = provider.complete(request).await?;
    println!("Response: {:?}", response.message.content);
    println!(
        "Tokens: {} in / {} out",
        response.usage.input_tokens, response.usage.output_tokens
    );

    Ok(())
}
