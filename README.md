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

### The Problem with Traditional Grep

`grep` and `ripgrep` are brute-force tools — they scan every byte of every file on every search. On a 9,000-file codebase, even ripgrep (with SIMD-accelerated regex) takes 80-100ms because it must read every file from disk.

rsgrep takes a different approach: **build an index once, then use it to skip 99% of files**.

### Sparse N-gram Indexing

The core idea comes from [Cursor's blog post](https://cursor.com/blog/fast-regex-search) on how they built fast search for their IDE. Instead of fixed-length trigrams (3-character substrings), rsgrep uses **sparse n-grams** — variable-length substrings selected by a deterministic weight function.

#### The Weight Table

rsgrep ships with a static 256x256 byte-pair weight table. Each entry `W[a][b]` assigns a weight to the byte pair `(a, b)`. In a production system, these weights would be derived from inverse frequency in a large open-source corpus (rare pairs get high weights, common pairs like `th` get low weights).

#### What Makes an N-gram "Sparse"

Given content bytes `b[0..n)`, define pair weights:

```
p[i] = W[b[i]][b[i + 1]]
```

A substring `b[l..r)` is a **sparse n-gram** if and only if the pair weights at both edges are **strictly greater** than every interior pair weight:

```
p[l] > max(p[l+1 .. r-3])
AND
p[r-2] > max(p[l+1 .. r-3])
```

For 2-byte and 3-byte substrings, the interior is empty, so the condition is vacuously true. This means sparse n-grams are naturally variable-length: rare byte combinations produce longer, more selective n-grams.

#### Structural Property: Non-Crossing (Laminarity)

Sparse n-grams form a **laminar family** — any two valid intervals are either disjoint, nested, or touching, but never crossing. This property is what makes the covering algorithm correct.

### Indexing (`rsgrep index`)

1. **Walk** the directory respecting `.gitignore` and `.rsgrep-ignore`, skip binary files (null-byte detection in first 8KB) and files over 10MB
2. **Extract sparse n-grams** using `build_all` — enumerate all valid sparse n-grams in each file (O(n) per file with a 64-byte cap on n-gram length)
3. **Build an inverted index** — for each unique n-gram, store a sorted posting list of file IDs that contain it
4. **Encode and store** — posting lists use delta-varint compression; the lookup table is a linear-probing hash table (load factor 0.7, 20 bytes per slot) designed to be memory-mapped

```
Input file: "fn handleExport() { ... }"

Pair weights:  [f,n]=42  [n, ]=8  [ ,h]=180  [h,a]=15  ...

Sparse n-grams extracted:
  "fn"        (2 bytes, vacuously valid)
  "fn "       (3 bytes, vacuously valid)
  " handleE"  (8 bytes, edges [' ',h]=180 and [l,e]=195 dominate interior)
  ...

Each n-gram hashed (xxh3) → posting list updated with this file's ID
```

### Searching (`rsgrep search`)

1. **Parse** the regex into a high-level IR using `regex-syntax`
2. **Extract mandatory literals** — walk the AST to find byte sequences that *must* appear in any match. `foo.*bar` yields `["foo", "bar"]`. Alternations like `foo|bar` produce branch-local literals.
3. **Compute covering set** — for each mandatory literal, run `build_covering` to find the minimum-cardinality set of sparse n-grams that covers every byte position. This uses a greedy interval-cover algorithm:
   - Start at the leftmost uncovered position
   - Among all valid sparse n-grams that cover this position, pick the one reaching farthest right
   - Advance past it and repeat
4. **Build query plan** — combine covering n-grams into a tree: AND for concatenated literals, OR for alternation branches
5. **Execute** — look up each n-gram's posting list from the mmap'd hash table, then intersect (AND) or union (OR) them
6. **Verify** — only the resulting candidate files (typically 5-10 out of thousands) are read and matched against the full regex

```
Query: "handleExport"

Step 1: Extract literal → "handleExport"
Step 2: build_covering → [" handleE", "Export"] (2 n-grams)
Step 3: Query plan → AND(lookup(" handleE"), lookup("Export"))
Step 4: Posting lists → {file_3, file_7, file_42} ∩ {file_7, file_42, file_100}
Step 5: Candidates → {file_7, file_42}
Step 6: Verify → read 2 files instead of 9,000
```

### Why It's Fast

| Approach | Work per search | On 9K files |
|----------|----------------|-------------|
| `grep` | Read every file, match regex | ~1,000ms |
| `ripgrep` | Read every file, SIMD regex | ~100ms |
| `rsgrep` | Hash lookup + read ~5 files | ~10-50ms |

The index turns an O(total bytes) problem into an O(candidate files) problem. For selective patterns (specific function names, unique strings), the candidate set is tiny — often just 2-5 files out of thousands.

The trade-off is a one-time index build cost (~22s for 9K files). This pays for itself after just a few searches.

### Recall Guarantee

rsgrep **never misses matches**. The index is a pre-filter: any file that matches the regex is guaranteed to appear in the candidate set (because it must contain all the mandatory literals, and therefore all the covering n-grams). False positives are possible (a file passes the index filter but doesn't match the regex), but false negatives are not. The final regex verification step ensures 100% correctness.

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
