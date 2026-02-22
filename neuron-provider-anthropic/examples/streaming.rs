//! Streaming example: real-time token output from Anthropic.
//!
//! Set ANTHROPIC_API_KEY in your environment and run:
//!   cargo run --example streaming -p neuron-provider-anthropic

use futures::StreamExt;
use neuron_provider_anthropic::Anthropic;
use neuron_types::{CompletionRequest, ContentBlock, Message, Provider, Role, StreamEvent};
use std::io::Write;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY environment variable must be set");

    let provider = Anthropic::new(api_key);

    let request = CompletionRequest {
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text(
                "Write a haiku about Rust programming.".into(),
            )],
        }],
        max_tokens: Some(256),
        ..Default::default()
    };

    let handle = provider.complete_stream(request).await?;
    let mut receiver = handle.receiver;

    print!("Streaming: ");
    std::io::stdout().flush()?;

    while let Some(event) = receiver.next().await {
        match event {
            StreamEvent::TextDelta(text) => {
                print!("{text}");
                std::io::stdout().flush()?;
            }
            StreamEvent::ThinkingDelta(thought) => {
                print!("[thinking: {thought}]");
                std::io::stdout().flush()?;
            }
            StreamEvent::Usage(usage) => {
                println!();
                println!(
                    "Token usage: {} input, {} output",
                    usage.input_tokens, usage.output_tokens
                );
            }
            StreamEvent::MessageComplete(_msg) => {
                println!();
                println!("Stream complete.");
            }
            StreamEvent::Error(err) => {
                eprintln!("\nStream error: {err}");
            }
            _ => {}
        }
    }

    Ok(())
}
