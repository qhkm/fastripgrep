# Contributing to frg

## Development Setup

```bash
git clone https://github.com/qhkm/fastripgrep
cd fastripgrep
cargo build
cargo test
```

## Running Tests

```bash
cargo test          # 101 tests
cargo clippy        # lint
cargo bench         # benchmarks
```

## Architecture

```
src/
  main.rs          # Entry point
  cli.rs           # CLI parsing + command dispatch
  lib.rs           # Module exports
  index/           # Indexing pipeline
    ngram.rs       # Sparse n-gram extraction (core algorithm)
    postings.rs    # Delta-varint posting lists
    lookup.rs      # Mmap'd hash table
    filetable.rs   # File ID <-> path mapping
    meta.rs        # Index metadata (JSON)
    mod.rs         # Build + update orchestration
  search/          # Search pipeline
    decompose.rs   # Regex -> query plan
    intersect.rs   # Posting list set operations
    verify.rs      # Candidate verification
    replace.rs     # Search & replace
    mod.rs         # Search orchestration + streaming
  output/          # Output formatting
    mod.rs         # Ripgrep-style + JSON
    color.rs       # Terminal colors
  ignore.rs        # File walking + gitignore
```

## Pull Requests

1. Fork the repo
2. Create a feature branch
3. Write tests for new functionality
4. Run `cargo test && cargo clippy -- -D warnings`
5. Submit a PR

## Release Process

See [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) for the full verification process.
