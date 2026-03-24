# rsgrep

Fast regex search with sparse n-gram indexing. Inspired by [Cursor's fast regex search](https://cursor.com/blog/fast-regex-search).

Instead of scanning every file like `grep` or `ripgrep`, rsgrep pre-builds an index of sparse n-grams and uses it to narrow candidates before running the regex — achieving sub-50ms searches on large codebases.

## Benchmarks

Tested on a real 9,000-file codebase (openclaw):

| Pattern | grep | ripgrep | rsgrep |
|---------|------|---------|--------|
| `renderUsage` (selective) | 1,467ms | 87ms | **49ms** |
| `import.*from.*react` (regex) | 1,055ms | 102ms | **20ms** |
| `function` (broad, ~24K hits) | 554ms | 84ms | 170ms |

- **30-53x faster than grep** for selective and regex searches
- **2-5x faster than ripgrep** for selective/regex patterns
- For broad patterns matching most files, ripgrep's SIMD engine is faster since both tools read every file anyway
- Index build: ~22s for 9K files (pays for itself after a few searches)

## Install

```bash
cargo install --path .
```

## Usage

```bash
# Build the index (required once, re-run after major changes)
rsgrep index [path]

# Search
rsgrep search <pattern> [path]

# Check index status
rsgrep status [path]

# Update index (currently does a full rebuild)
rsgrep update [path]
```

## Search Flags

| Flag | Description |
|------|-------------|
| `-i` | Case-insensitive |
| `-F` / `--literal` | Treat pattern as fixed string |
| `-l` | Files-with-matches only |
| `-c` / `--count` | Show match count per file |
| `-C <n>` / `--context <n>` | Show surrounding lines |
| `-m <n>` / `--max-count <n>` | Limit results per file |
| `-q` / `--quiet` | Suppress output, exit code only |
| `-n` / `--no-index` | Brute-force scan (no index) |
| `--glob <pattern>` | Filter files by glob |
| `--type <ext>` | Filter by file extension |
| `--json` | Output as JSON |

## How It Works

### Indexing (`rsgrep index`)

1. Walk the directory respecting `.gitignore` and `.rsgrep-ignore`
2. For each file, extract **sparse n-grams** — variable-length byte substrings whose edge pair-weights strictly dominate all interior pair-weights
3. Build an inverted index: n-gram hash → sorted list of file IDs
4. Store as two mmap-friendly binary files: `postings.bin` (delta-varint encoded posting lists) + `lookup.bin` (linear-probing hash table)

### Searching (`rsgrep search`)

1. Parse the regex into an AST using `regex-syntax`
2. Extract mandatory literals and decompose them into a **covering set** of sparse n-grams using a minimum-cardinality interval-cover greedy
3. Build a query plan: AND for concatenations, OR for alternations
4. Look up posting lists and intersect/union them to find candidate files
5. Verify candidates with the full regex (only a handful of files, not thousands)

### Why It's Fast

Traditional grep tools scan every file on every search — O(total bytes). rsgrep's index reduces this to O(candidate files), which for selective patterns means reading 5-10 files instead of 9,000.

## Architecture

```
.rsgrep/
├── CURRENT                    # Active generation ID
└── generations/<id>/
    ├── meta.json              # Version, commit hash, file count
    ├── postings.bin           # Delta-varint posting lists
    ├── lookup.bin             # Mmap'd hash table (20-byte slots, 0.7 load factor)
    └── files.bin              # File ID → path mapping
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Match found |
| 1 | No match found |
| 2 | Error |

## Development

```bash
cargo test          # 73 tests
cargo bench         # Criterion benchmarks
cargo clippy        # Lint
```

## License

MIT
