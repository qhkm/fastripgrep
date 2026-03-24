use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use fastripgrep::search::SearchOptions;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

const MAX_FILESIZE: u64 = 10 * 1024 * 1024;
const SMALL_REPO_FILES: usize = 1_000;
const MEDIUM_REPO_FILES: usize = 5_000;

struct RepoFixture {
    dir: TempDir,
    selective_pattern: String,
    broad_pattern: &'static str,
    alternation_pattern: String,
    scan_all_pattern: &'static str,
}

impl RepoFixture {
    fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn create_synthetic_repo(num_files: usize) -> RepoFixture {
    let dir = TempDir::new().unwrap();
    let broad_pattern = "shared_symbol";
    let selective_index = num_files / 2;
    let selective_pattern = format!("needle_target_{selective_index}");
    let alternation_pattern = format!(
        "needle_target_{selective_index}|needle_target_{}",
        selective_index / 2
    );
    let scan_all_pattern = ".*";

    for i in 0..num_files {
        let shard = format!("shard_{:03}", i % 32);
        let module_dir = dir.path().join("src").join(shard);
        fs::create_dir_all(&module_dir).unwrap();

        let maybe_target = if i == selective_index {
            format!("\nconst {}: &str = \"present\";\n", selective_pattern)
        } else {
            String::new()
        };

        let content = format!(
            "pub fn function_{i}() -> usize {{\n    let value_{i} = {i};\n    let label = \"{broad_pattern}\";\n    println!(\"{{}}:{{}}\", label, value_{i});{maybe_target}    value_{i} * 2\n}}\n"
        );

        fs::write(module_dir.join(format!("file_{i:05}.rs")), content).unwrap();
    }

    RepoFixture {
        dir,
        selective_pattern,
        broad_pattern,
        alternation_pattern,
        scan_all_pattern,
    }
}

fn build_search_opts(no_index: bool) -> SearchOptions {
    SearchOptions {
        no_index,
        ..SearchOptions::default()
    }
}

fn bench_index_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("index_build");

    for num_files in [SMALL_REPO_FILES, MEDIUM_REPO_FILES] {
        group.throughput(Throughput::Elements(num_files as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_files),
            &num_files,
            |b, &files| {
                let fixture = create_synthetic_repo(files);
                b.iter(|| {
                    let _ = fs::remove_dir_all(fixture.path().join(".rsgrep"));
                    fastripgrep::index::build_index(black_box(fixture.path()), MAX_FILESIZE).unwrap();
                });
            },
        );
    }

    group.finish();
}

fn bench_indexed_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_indexed");

    for num_files in [SMALL_REPO_FILES, MEDIUM_REPO_FILES] {
        let fixture = create_synthetic_repo(num_files);
        fastripgrep::index::build_index(fixture.path(), MAX_FILESIZE).unwrap();
        let indexed_opts = build_search_opts(false);

        group.throughput(Throughput::Elements(num_files as u64));

        group.bench_with_input(
            BenchmarkId::new("selective", num_files),
            &fixture,
            |b, fixture| {
                b.iter(|| {
                    fastripgrep::search::search(
                        black_box(fixture.path()),
                        black_box(&fixture.selective_pattern),
                        black_box(&indexed_opts),
                    )
                    .unwrap()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("broad", num_files),
            &fixture,
            |b, fixture| {
                b.iter(|| {
                    fastripgrep::search::search(
                        black_box(fixture.path()),
                        black_box(fixture.broad_pattern),
                        black_box(&indexed_opts),
                    )
                    .unwrap()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("alternation", num_files),
            &fixture,
            |b, fixture| {
                b.iter(|| {
                    fastripgrep::search::search(
                        black_box(fixture.path()),
                        black_box(&fixture.alternation_pattern),
                        black_box(&indexed_opts),
                    )
                    .unwrap()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("scan_all", num_files),
            &fixture,
            |b, fixture| {
                b.iter(|| {
                    fastripgrep::search::search(
                        black_box(fixture.path()),
                        black_box(fixture.scan_all_pattern),
                        black_box(&indexed_opts),
                    )
                    .unwrap()
                });
            },
        );
    }

    group.finish();
}

fn bench_bruteforce_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_bruteforce");

    for num_files in [SMALL_REPO_FILES, MEDIUM_REPO_FILES] {
        let fixture = create_synthetic_repo(num_files);
        let brute_opts = build_search_opts(true);

        group.throughput(Throughput::Elements(num_files as u64));

        group.bench_with_input(
            BenchmarkId::new("selective", num_files),
            &fixture,
            |b, fixture| {
                b.iter(|| {
                    fastripgrep::search::search(
                        black_box(fixture.path()),
                        black_box(&fixture.selective_pattern),
                        black_box(&brute_opts),
                    )
                    .unwrap()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("broad", num_files),
            &fixture,
            |b, fixture| {
                b.iter(|| {
                    fastripgrep::search::search(
                        black_box(fixture.path()),
                        black_box(fixture.broad_pattern),
                        black_box(&brute_opts),
                    )
                    .unwrap()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("alternation", num_files),
            &fixture,
            |b, fixture| {
                b.iter(|| {
                    fastripgrep::search::search(
                        black_box(fixture.path()),
                        black_box(&fixture.alternation_pattern),
                        black_box(&brute_opts),
                    )
                    .unwrap()
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("scan_all", num_files),
            &fixture,
            |b, fixture| {
                b.iter(|| {
                    fastripgrep::search::search(
                        black_box(fixture.path()),
                        black_box(fixture.scan_all_pattern),
                        black_box(&brute_opts),
                    )
                    .unwrap()
                });
            },
        );
    }

    group.finish();
}

fn configure_criterion() -> Criterion {
    Criterion::default().sample_size(10)
}

criterion_group!(
    name = benches;
    config = configure_criterion();
    targets = bench_index_build, bench_indexed_search, bench_bruteforce_search
);
criterion_main!(benches);
