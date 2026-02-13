use agent_lib::token::{TokenCounter, TruncationPolicy, count_tokens};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

const LONG_TEXT_LEN: usize = 10_000;
const BATCH_SIZE: usize = 100;
const HISTORY_SIZE: usize = 1_000;

fn bench_count_tokens(c: &mut Criterion) {
    let text = "a".repeat(LONG_TEXT_LEN);
    c.bench_function("count_tokens_10k_chars", |b| {
        b.iter(|| count_tokens(black_box(&text)))
    });
}

fn bench_batch_count(c: &mut Criterion) {
    let counter = TokenCounter::new();
    let texts: Vec<String> = (0..BATCH_SIZE).map(|i| format!("Message {}", i)).collect();
    let texts_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

    c.bench_function("batch_100_messages", |b| {
        b.iter(|| counter.count_batch(black_box(&texts_refs)))
    });
}

fn bench_cache_effectiveness(c: &mut Criterion) {
    let mut history = agent_lib::session::ConversationHistory::new();

    for i in 0..HISTORY_SIZE {
        history.push(agent_lib::model::Message::user(format!("Message {}", i)));
    }

    c.bench_function("cached_total_tokens", |b| b.iter(|| history.total_tokens()));
}

fn bench_truncation_policy_for_model(c: &mut Criterion) {
    let models = [
        "gpt-4",
        "gpt-4-turbo",
        "gpt-3.5-turbo",
        "claude-3-haiku",
        "gemini-pro-1.5",
    ];

    for model in models {
        c.bench_with_input(
            BenchmarkId::new("for_model", model),
            &model,
            |b, model_name| b.iter(|| TruncationPolicy::for_model(black_box(*model_name))),
        );
    }
}

fn bench_truncation_policy_methods(c: &mut Criterion) {
    let policy = TruncationPolicy::tokens(10000);

    c.bench_function("exceeds", |b| b.iter(|| policy.exceeds(black_box(5000))));

    c.bench_function("remaining", |b| {
        b.iter(|| policy.remaining(black_box(3000)))
    });

    c.bench_function("has_enough", |b| {
        b.iter(|| policy.has_enough(black_box(2000)))
    });

    c.bench_function("estimate_message_capacity", |b| {
        b.iter(|| policy.estimate_message_capacity(black_box(100)))
    });
}

fn bench_token_counter_new(c: &mut Criterion) {
    c.bench_function("token_counter_new", |b| b.iter(TokenCounter::new));
}

fn bench_token_counter_mode_name(c: &mut Criterion) {
    let counter = TokenCounter::new();

    c.bench_function("token_counter_mode_name", |b| {
        b.iter(|| counter.mode_name())
    });
}

fn bench_token_counter_estimate(c: &mut Criterion) {
    let counter = TokenCounter::new();

    c.bench_function("token_counter_estimate_from_bytes", |b| {
        b.iter(|| counter.estimate_from_bytes(black_box(123456)))
    });
}

criterion_group!(
    benches,
    bench_count_tokens,
    bench_batch_count,
    bench_cache_effectiveness,
    bench_truncation_policy_for_model,
    bench_truncation_policy_methods,
    bench_token_counter_new,
    bench_token_counter_mode_name,
    bench_token_counter_estimate
);
criterion_main!(benches);
