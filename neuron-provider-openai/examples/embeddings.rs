//! Example: generate embeddings using the OpenAI Embeddings API.
//!
//! Requires OPENAI_API_KEY environment variable.
//!
//! Run with: OPENAI_API_KEY=sk-... cargo run --example embeddings -p neuron-provider-openai

use neuron_provider_openai::OpenAi;
use neuron_types::{EmbeddingProvider, EmbeddingRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = OpenAi::from_env()?;

    // Embed a batch of texts
    let request = EmbeddingRequest {
        model: "text-embedding-3-small".to_string(),
        input: vec![
            "Rust is a systems programming language".to_string(),
            "Python is great for data science".to_string(),
            "TypeScript powers modern web development".to_string(),
        ],
        dimensions: Some(256), // Reduce dimensions for efficiency
        ..Default::default()
    };

    let response = client.embed(request).await?;

    println!("Model: {}", response.model);
    println!("Embeddings: {}", response.embeddings.len());
    println!("Dimensions: {}", response.embeddings[0].len());
    println!(
        "Usage: {} prompt tokens, {} total tokens",
        response.usage.prompt_tokens, response.usage.total_tokens,
    );

    // Compute cosine similarity between first two embeddings
    let sim = cosine_similarity(&response.embeddings[0], &response.embeddings[1]);
    println!("\nSimilarity (Rust vs Python): {sim:.4}");

    let sim = cosine_similarity(&response.embeddings[0], &response.embeddings[2]);
    println!("Similarity (Rust vs TypeScript): {sim:.4}");

    Ok(())
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm_a * norm_b)
}
