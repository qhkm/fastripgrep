use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

fn setup_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("main.rs"),
        "fn main() {\n    let config = load_config();\n    run_server(config);\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("config.rs"),
        "pub struct Config {\n    pub port: u16,\n}\npub fn load_config() -> Config {\n    Config { port: 8080 }\n}\n",
    )
    .unwrap();
    fs::write(
        src.join("server.rs"),
        "pub fn run_server(config: crate::config::Config) {\n    println!(\"{}:{}\", config.host, config.port);\n}\n",
    )
    .unwrap();
    fs::write(dir.path().join("image.png"), &[0x89, 0x50, 0x4E, 0x47, 0x00]).unwrap();
    dir
}

#[test]
fn test_full_pipeline() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions::default();
    let results = rsgrep::search::search(dir.path(), "load_config", &opts).unwrap();
    assert!(
        results.len() >= 2,
        "load_config should appear in main.rs and config.rs, got {}",
        results.len()
    );
}

#[test]
fn test_search_superset_of_bruteforce() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions::default();
    let indexed = rsgrep::search::search(dir.path(), "Config", &opts).unwrap();
    let indexed_files: HashSet<_> = indexed.iter().map(|m| m.file_path.clone()).collect();

    // Brute force
    let bf_opts = rsgrep::search::SearchOptions {
        no_index: true,
        ..Default::default()
    };
    let brute = rsgrep::search::search(dir.path(), "Config", &bf_opts).unwrap();
    let bf_files: HashSet<_> = brute.iter().map(|m| m.file_path.clone()).collect();

    for f in &bf_files {
        assert!(indexed_files.contains(f), "index missed file {}", f);
    }
}

#[test]
fn test_binary_file_excluded() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions::default();
    let results = rsgrep::search::search(dir.path(), "PNG", &opts).unwrap();
    for m in &results {
        assert!(!m.file_path.contains("image.png"));
    }
}

#[test]
fn test_case_insensitive() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions {
        case_insensitive: true,
        ..Default::default()
    };
    let results = rsgrep::search::search(dir.path(), "config", &opts).unwrap();
    assert!(!results.is_empty());
}

#[test]
fn test_literal_search() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions {
        literal: true,
        ..Default::default()
    };
    let results = rsgrep::search::search(dir.path(), "{}:{}", &opts).unwrap();
    assert!(!results.is_empty());
}

#[test]
fn test_alternation_search() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions::default();
    let results =
        rsgrep::search::search(dir.path(), "load_config|run_server", &opts).unwrap();
    assert!(
        results.len() >= 3,
        "alternation should find both functions, got {}",
        results.len()
    );
}

#[test]
fn test_binary_excluded_on_scanall_fallback() {
    // ScanAll patterns (wildcard, single char) should NOT return binary file matches
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();

    // ".*" triggers ScanAll — uses all_file_ids()
    let opts = rsgrep::search::SearchOptions::default();
    let results = rsgrep::search::search(dir.path(), ".*", &opts).unwrap();
    for m in &results {
        assert!(
            !m.file_path.contains("image.png"),
            "binary file should not appear in ScanAll results"
        );
    }

    // Single char "P" also triggers ScanAll (no 2-byte literal)
    let results = rsgrep::search::search(dir.path(), "P", &opts).unwrap();
    for m in &results {
        assert!(
            !m.file_path.contains("image.png"),
            "binary file should not appear in single-char ScanAll results"
        );
    }
}

#[test]
fn test_binary_excluded_in_no_index_mode() {
    let dir = setup_project();
    // No index needed — brute-force mode
    let opts = rsgrep::search::SearchOptions {
        no_index: true,
        ..Default::default()
    };
    let results = rsgrep::search::search(dir.path(), "PNG", &opts).unwrap();
    for m in &results {
        assert!(
            !m.file_path.contains("image.png"),
            "binary file should be excluded in --no-index mode too"
        );
    }
}

#[test]
fn test_indexed_and_bruteforce_same_results() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();

    let indexed = rsgrep::search::search(
        dir.path(),
        "config",
        &rsgrep::search::SearchOptions::default(),
    )
    .unwrap();
    let brute = rsgrep::search::search(
        dir.path(),
        "config",
        &rsgrep::search::SearchOptions {
            no_index: true,
            ..Default::default()
        },
    )
    .unwrap();

    // Same file set
    let idx_files: HashSet<_> = indexed.iter().map(|m| &m.file_path).collect();
    let bf_files: HashSet<_> = brute.iter().map(|m| &m.file_path).collect();
    assert_eq!(idx_files, bf_files, "indexed and brute-force should find same files");

    // Same match count
    assert_eq!(
        indexed.len(),
        brute.len(),
        "indexed and brute-force should find same number of matches"
    );
}
