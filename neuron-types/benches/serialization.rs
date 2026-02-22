use criterion::{Criterion, black_box, criterion_group, criterion_main};
use neuron_types::*;

fn make_message(n_blocks: usize) -> Message {
    Message {
        role: Role::User,
        content: (0..n_blocks)
            .map(|i| ContentBlock::Text(format!("Block {i} with some content")))
            .collect(),
    }
}

fn make_request(n_messages: usize) -> CompletionRequest {
    CompletionRequest {
        model: "test-model".to_string(),
        messages: (0..n_messages).map(|_| make_message(3)).collect(),
        ..Default::default()
    }
}

fn bench_message_serialize(c: &mut Criterion) {
    let msg = make_message(5);
    c.bench_function("message_serialize_5_blocks", |b| {
        b.iter(|| serde_json::to_string(black_box(&msg)).unwrap())
    });
}

fn bench_request_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("completion_request_serialize");
    for n in [10, 100, 1000] {
        let req = make_request(n);
        group.bench_function(format!("{n}_messages"), |b| {
            b.iter(|| serde_json::to_string(black_box(&req)).unwrap())
        });
    }
    group.finish();
}

criterion_group!(benches, bench_message_serialize, bench_request_serialize);
criterion_main!(benches);
