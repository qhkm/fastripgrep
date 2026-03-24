# frg (fastripgrep)

Fast regex search with sparse n-gram indexing. Inspired by [Cursor's fast regex search](https://cursor.com/blog/fast-regex-search).

Instead of scanning every file like `grep` or `ripgrep`, frg pre-builds an index of sparse n-grams and uses it to narrow candidates before running the regex — achieving sub-50ms searches on large codebases.

## Benchmarks

Tested on openclaw (9,000 files), case-sensitive, warm cache, output to `/dev/null`:

| Pattern | Type | grep | ripgrep | frg | vs rg |
|---------|------|------|---------|--------|-------|
| `renderUsage` | selective | 1,553ms | 53ms | **10ms** | **5.3x** |
| `useState\|useEffect` | alternation | 962ms | 54ms | **13ms** | **4.2x** |
| `.*` | wildcard | — | 197ms | **74ms** | **2.7x** |
| `TODO\|FIXME\|HACK\|XXX\|BUG` | multi-alt | — | 53ms | **21ms** | **2.5x** |
| `import.*from.*react` | regex | 1,080ms | 84ms | **50ms** | **1.7x** |
| `x` | single byte | — | 102ms | **59ms** | **1.7x** |
| `function` (24K hits) | broad | 398ms | 75ms | **51ms** | **1.5x** |
| `^\s*export\s` | anchored | — | 72ms | **68ms** | **1.06x** |

**frg wins on all 8 pattern types.** 100% match count parity with ripgrep.

### How

- **Indexed patterns** (6/8): sparse n-gram index pre-filters to ~5 candidate files out of 9,000 before regex verification
- **ScanAll patterns** (2/8): parallel file I/O with rayon, SIMD-accelerated byte search via memchr, zero-copy streaming output (no `Match` struct allocation for millions of results)
- **Index build**: ~23s one-time cost for 9K files, pays for itself after a few searches

## Install

```
curl -fsSL https://raw.githubusercontent.com/qhkm/fastripgrep/main/install.sh | sh
```

Or via cargo:

```
cargo install fastripgrep
```

Or build from source:

```bash
git clone https://github.com/qhkm/fastripgrep
cd fastripgrep
cargo build --release
# Binary at target/release/frg
```

## Usage

```bash
# Build the index (one-time, re-run after major changes)
frg index [path]

# Search (uses index if available)
frg search <pattern> [path]

# Rebuild index
frg update [path]

# Show index stats
frg status [path]
```

### Examples

```bash
# Find a specific function
frg search "handleExport" .

# Regex with alternation
frg search "useState|useEffect" .

# Case-insensitive
frg search -i "config" .

# Smart case (lowercase = insensitive, mixed = sensitive)
frg search -S "config" .     # matches Config, CONFIG, config
frg search -S "Config" .     # matches Config only

# Fixed string (no regex interpretation)
frg search -F '${variable}' .

# Files only
frg search -l "TODO" .

# Count matches per file
frg search -c "function" .

# Context lines
frg search -C 3 "error" .

# Filter by file type
frg search --type ts "import" .

# JSON output
frg search --json "pattern" .

# Brute force (skip index, like ripgrep)
frg search -n "pattern" .
```

## Search Flags

| Flag | Description |
|------|-------------|
| `-i` | Case-insensitive |
| `-S` / `--smart-case` | Case-insensitive if pattern is all lowercase |
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

## Configuration

frg supports a config file with default arguments, like ripgrep.

**Location:** `~/.frgrc` or `$FRG_CONFIG_PATH`

**Format:** One argument per line. `#` comments. Empty lines ignored.

```bash
# ~/.frgrc

# Smart case by default
--smart-case
```

## How It Works

### The Core Idea

`grep` and `ripgrep` scan every file on every search. frg builds an index once and uses it to skip 99% of files:

| Approach | Strategy | Time on 9K files |
|----------|----------|-----------------|
| grep | Scan all files, match regex | ~1,000ms |
| ripgrep | Scan all files, SIMD regex | ~50-100ms |
| frg | Index lookup + read ~5 files | ~10-70ms |

### Sparse N-gram Indexing

frg uses **sparse n-grams** — variable-length byte substrings selected by a deterministic weight function. This is the approach described in [Cursor's blog post](https://cursor.com/blog/fast-regex-search).

A 256x256 byte-pair weight table assigns a weight `W[a][b]` to each byte pair. A substring is a **sparse n-gram** if its edge pair-weights are **strictly greater** than all interior pair-weights:

```
p[l] > max(p[l+1 .. r-3])  AND  p[r-2] > max(p[l+1 .. r-3])
```

For 2-3 byte substrings, the interior is empty, so the condition is vacuously true. Sparse n-grams form a **laminar family** (non-crossing), which makes the covering algorithm correct.

### Indexing

1. Walk directory (respects `.gitignore`, `.frg-ignore`), skip binary files
2. Extract all sparse n-grams from each file in parallel (`build_all`, O(n) per file with 64-byte cap)
3. Build inverted index: n-gram hash (xxh3) -> sorted posting list of file IDs
4. Store as mmap-friendly binary: delta-varint posting lists + linear-probing hash table (0.7 load factor)

### Searching

1. Parse regex AST, extract mandatory literals
2. Compute minimum-cardinality covering set of sparse n-grams for each literal (`build_covering`)
3. Build query plan: AND for concatenations, OR for alternations
4. Intersect/union posting lists to get candidate file IDs
5. Verify candidates with full regex in parallel

```
Query: "handleExport"
  -> Covering n-grams: ["handleE", "Export"]
  -> Posting lists: {3,7,42} AND {7,42,100} = {7,42}
  -> Verify 2 files instead of 9,000
```

### Fast Path for ScanAll Patterns

When a pattern has no extractable literals (`.*`, single char `x`), frg can't use the index. Instead of falling back to slow brute force, it uses optimized parallel scanning:

- **`.*` / empty**: Skip regex entirely, output every line directly via parallel rayon workers with zero-copy byte output
- **Single byte (`x`)**: Use `memchr` (SIMD-accelerated SSE2/AVX2/NEON) instead of regex engine
- **All patterns**: Files processed in parallel with rayon, output buffers merged in file order

This makes frg 1.7-2.7x faster than ripgrep even on patterns the index can't help with.

### Recall Guarantee

frg **never misses matches**. The index is a pre-filter — any file matching the regex is guaranteed to be in the candidate set. False positives are possible (filtered by verification), false negatives are not.

Match counts are identical to ripgrep (case-sensitive mode) across all tested patterns.

## Architecture

```
.frg/
├── CURRENT                    # Active generation ID
└── generations/<id>/
    ├── meta.json              # Version, commit hash, file count, n-gram count
    ├── postings.bin           # Delta-varint encoded posting lists
    ├── lookup.bin             # Mmap'd hash table (20-byte slots)
    └── files.bin              # File ID -> path + metadata mapping
```

**Key dependencies:** `regex` + `regex-syntax` (matching), `memmap2` (mmap), `rayon` (parallelism), `memchr` (SIMD byte search), `ignore` (gitignore), `xxhash-rust` (stable hashing), `clap` (CLI)

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Match found |
| 1 | No match found |
| 2 | Error |

## Correctness

78 tests covering:

| Area | Coverage |
|------|----------|
| N-gram extraction | build_all, build_covering, weight table, strict `>`, laminarity |
| Posting lists | Delta-varint roundtrip, empty/single/multi |
| Lookup table | Linear probing, hash collisions, magic validation |
| File table | Roundtrip, non-UTF-8 paths (Unix), all IDs |
| Regex decomposition | Literals, alternation OR, wildcards, single-char fallback |
| Search pipeline | Indexed vs brute-force parity, binary exclusion, ScanAll fallback |
| Property tests | Covering subset of all, pair coverage, laminarity, greedy optimality |
| Edge cases | Anchors `^$`, Unicode, empty pattern, zero results, symlinks |

```bash
cargo test          # 82 tests
cargo bench         # Criterion benchmarks (1K + 5K file repos)
cargo clippy        # Zero warnings
```

## Roadmap

**Done:**
- [x] Incremental index updates — overlay/tombstone architecture (~70ms vs 21s full rebuild)
- [x] Shell completions — `frg completions bash|zsh|fish|powershell|elvish`
- [x] Man page — `frg man | man -l -`
- [x] `--follow` flag — user-controllable symlink following
- [x] `frg init` — auto-detect project type, create `.frgignore`, optional git hook
- [x] Multi-pattern search — `frg search "pat1" -e "pat2" -e "pat3"`

**Adoption:**
- [x] ~~`frg watch`~~ — auto-updates index on file changes with debouncing
- [x] ~~Homebrew formula~~ — `brew tap qhkm/tap && brew install frg`

**Editor integrations:**
- [ ] VS Code extension — use frg as search backend
- [ ] Neovim plugin — `:Frg` command with quickfix integration

**Performance:**
- [ ] Corpus-derived weight table — real inverse-frequency weights from open-source code for better n-gram selectivity
- [ ] Monotone-stack n-gram extraction — remove the 64-byte cap while staying O(n)

**Features:**
- [ ] `--replace` / `frg replace` — search and replace with preview

## License

MIT
