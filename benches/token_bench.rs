use agent_lib::token::{TokenCounter, TruncationPolicy, approx_token_count, tiktoken_count};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

fn bench_approx_count(c: &mut Criterion) {
    let text = "a".repeat(10000);
    c.bench_function("approx_10k_chars", |b| {
        b.iter(|| approx_token_count(black_box(&text)))
    });
}

fn bench_tiktoken_count(c: &mut Criterion) {
    let text = "a".repeat(10000);
    c.bench_function("tiktoken_10k_chars", |b| {
        b.iter(|| tiktoken_count(black_box(&text)))
    });
}

fn bench_batch_count(c: &mut Criterion) {
    let counter = TokenCounter::auto();
    let texts: Vec<String> = (0..100).map(|i| format!("Message {}", i)).collect();
    let texts_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

    c.bench_function("batch_100_messages", |b| {
        b.iter(|| counter.count_batch(black_box(&texts_refs)))
    });
}

fn bench_cache_effectiveness(c: &mut Criterion) {
    let mut history = agent_lib::session::ConversationHistory::new();

    // 添加1000条消息
    for i in 0..1000 {
        history.push(agent_lib::model::Message::user(format!("Message {}", i)));
    }

    c.bench_function("cached_total_tokens", |b| b.iter(|| history.total_tokens()));
}

fn bench_truncation_policy_for_model(c: &mut Criterion) {
    let models = vec![
        "gpt-4",
        "gpt-4-turbo",
        "gpt-3.5-turbo",
        "claude-3-haiku",
        "gemini-pro-1.5",
    ];

    for model in models {
        let policy = TruncationPolicy::for_model(model);
        c.bench_with_input(BenchmarkId::new("for_model", model), model, |b, _| {
            b.iter(|| TruncationPolicy::for_model(black_box(model)))
        });
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

fn bench_token_counter_auto(c: &mut Criterion) {
    c.bench_function("token_counter_auto", |b| b.iter(|| TokenCounter::auto()));
}

fn bench_token_counter_mode_names(c: &mut Criterion) {
    let counter_approx = TokenCounter::with_approx();
    let counter_tiktoken = TokenCounter::with_tiktoken();

    c.bench_function("approx_mode_name", |b| {
        b.iter(|| counter_approx.mode_name())
    });

    c.bench_function("tiktoken_mode_name", |b| {
        b.iter(|| counter_tiktoken.mode_name())
    });
}

criterion_group!(
    benches,
    bench_approx_count,
    bench_tiktoken_count,
    bench_batch_count,
    bench_cache_effectiveness,
    bench_truncation_policy_for_model,
    bench_truncation_policy_methods,
    bench_token_counter_auto,
    bench_token_counter_mode_names
);
criterion_main!(benches);
