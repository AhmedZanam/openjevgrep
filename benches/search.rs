use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ojg_core::chunk::fallback_chunks;
use ojg_core::scanner::ScannedFile;

fn fallback_chunking(c: &mut Criterion) {
    let file = ScannedFile {
        path: "fixture.rs".into(),
        relative_path: "fixture.rs".into(),
        language: Some("rust".to_string()),
        content: "fn validate() {\n    true\n}\n".repeat(100),
    };
    c.bench_function("fallback_chunking", |bench| {
        bench.iter(|| fallback_chunks(black_box(&file), 160, 2))
    });
}

criterion_group!(search_benches, fallback_chunking);
criterion_main!(search_benches);
