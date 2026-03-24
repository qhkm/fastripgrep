use criterion::{criterion_group, criterion_main, Criterion};
use std::fs;
use tempfile::TempDir;

fn create_synthetic_repo(num_files: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    for i in 0..num_files {
        let content = format!(
            "fn function_{}() {{\n    let value_{} = {};\n    println!(\"hello {}\");\n}}\n",
            i,
            i,
            i * 42,
            i
        );
        fs::write(dir.path().join(format!("file_{}.rs", i)), content).unwrap();
    }
    dir
}

fn bench_index_build(c: &mut Criterion) {
    let dir = create_synthetic_repo(1000);
    c.bench_function("index_build_1k", |b| {
        b.iter(|| {
            let _ = fs::remove_dir_all(dir.path().join(".rsgrep"));
            rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
        })
    });
}

fn bench_search(c: &mut Criterion) {
    let dir = create_synthetic_repo(1000);
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions::default();

    c.bench_function("search_selective_1k", |b| {
        b.iter(|| rsgrep::search::search(dir.path(), "function_500", &opts).unwrap())
    });

    c.bench_function("search_broad_1k", |b| {
        b.iter(|| rsgrep::search::search(dir.path(), "fn ", &opts).unwrap())
    });
}

criterion_group!(benches, bench_index_build, bench_search);
criterion_main!(benches);
