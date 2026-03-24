use criterion::black_box;
use criterion::criterion_group;
use criterion::criterion_main;

fn bench_placeholder(c: &mut criterion::Criterion) {
    c.bench_function("placeholder", |b| {
        b.iter(|| black_box(1 + 1))
    });
}

criterion_group!(benches, bench_placeholder);
criterion_main!(benches);
