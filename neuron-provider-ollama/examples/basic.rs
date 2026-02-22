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
        model: String::new(), // use default (llama3.2)
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
    println!(
        "Tokens: {} in / {} out",
        response.usage.input_tokens, response.usage.output_tokens
    );

    Ok(())
}
