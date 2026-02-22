use criterion::{Criterion, black_box, criterion_group, criterion_main};
use neuron_context::TokenCounter;
use neuron_types::*;

fn make_conversation(n: usize) -> Vec<Message> {
    (0..n)
        .map(|i| Message {
            role: if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            },
            content: vec![ContentBlock::Text(format!(
                "Message {i}: This is a moderately sized message with enough content \
                 to be realistic for token counting benchmarks."
            ))],
        })
        .collect()
}

fn bench_token_counting(c: &mut Criterion) {
    let counter = TokenCounter::default();
    let mut group = c.benchmark_group("token_count");
    for n in [100, 1000, 10000] {
        let msgs = make_conversation(n);
        group.bench_function(format!("{n}_messages"), |b| {
            b.iter(|| counter.estimate_messages(black_box(&msgs)))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_token_counting);
criterion_main!(benches);
