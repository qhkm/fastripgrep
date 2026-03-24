# rsgrep Design Spec

A Rust CLI grep alternative using sparse n-gram indexing for fast regex search across codebases of any size. Inspired by [Cursor's fast regex search](https://cursor.com/blog/fast-regex-search).

## Problem

Tools like ripgrep scan every file on every search. For large codebases (100K+ files), this means multi-second searches. By pre-building an index of sparse n-grams, we can narrow the candidate set to a handful of files before running the regex, achieving sub-100ms searches.

## CLI Interface

```
rsgrep index [path]          # Build/rebuild index for a directory (default: cwd)
rsgrep search <pattern>      # Regex search using the index
rsgrep update [path]         # Incrementally update index
rsgrep status                # Show index stats (file count, size, staleness)
```

### Flags

| Flag | Description |
|------|-------------|
| `--no-index` / `-n` | Fall back to brute-force scan |
| `--literal` / `-F` | Treat pattern as fixed string |
| `-i` | Case-insensitive |
| `-l` | Files-with-matches only |
| `--glob <pattern>` | Filter files by glob |
| `--type <ext>` | Filter by file type |
| `--context <n>` / `-C` | Show surrounding lines |
| `--json` | Output as JSON |
| `--count` / `-c` | Show match count per file |
| `--max-count <n>` / `-m` | Limit results per file |
| `--quiet` / `-q` | Suppress output, exit code only |

Output format: `file:line:content` with terminal color highlighting (ripgrep style).

Index stored in `.rsgrep/` at the root of the indexed directory.

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Match found |
| 1 | No match found |
| 2 | Error (missing index, I/O error, invalid regex) |

## Indexing Architecture

### Storage Layout

```
.rsgrep/
├── CURRENT            # Active generation ID
└── generations/
    └── <gen>/
        ├── meta.json       # Version, base commit hash, timestamp, counts
        ├── postings.bin    # Base posting lists for indexed n-grams
        ├── lookup.bin      # Base hash table: n-gram hash -> posting offset
        ├── files.bin       # Base file table: file ID -> (path, oid/mtime, size)
        └── overlay/
            ├── postings.bin   # Append-only postings for dirty/untracked files
            ├── lookup.bin     # Overlay hash table
            ├── files.bin      # Overlay file table
            └── tombstones.bin # Base file IDs hidden by the overlay
```

### Sparse N-gram Extraction

This section follows the Cursor article's sparse n-gram model rather than the earlier content-defined-chunking approximation.

**Weight table**: A static 256x256 byte-pair lookup array shipped with the binary. Each entry `W[a][b]` is the deterministic weight of byte pair `(a, b)`. In the final system this should be derived from inverse frequency in a large open-source corpus so rare pairs receive high weights and common pairs receive low weights.

Comparisons are strictly ordered:

- A larger weight is always better than a smaller weight.
- Equal weights are **not** dominating; a longer sparse n-gram requires both edges to be strictly greater than every interior weight.
- Two-byte and three-byte n-grams are intentionally allowed. They have no interior pair weights, so the sparse predicate is vacuously true.
- One-byte literals produce no sparse n-grams and therefore cannot constrain the index directly; they are handled only by final regex verification or full-scan fallback.

For content bytes `b[0..n)`, define pair weights:

```
p[i] = W[b[i]][b[i + 1]]    for 0 <= i < n - 1
```

A substring `b[l..r)` with `r - l >= 2` is a **sparse n-gram** iff the pair weights at its two ends are both strictly greater than every pair weight inside it:

```
p[l] > max(p[l + 1 .. r - 2])
AND
p[r - 2] > max(p[l + 1 .. r - 2])
```

When `r - l <= 3`, the interior is empty and the predicate is vacuously true.

This is the key invariant from the article: sparse n-grams are substrings whose two edge weights dominate all interior weights.

#### `build_all`

At index time we enumerate **all** sparse n-grams in each file and add the file ID to the posting list for each distinct n-gram present in that file.

```
function build_all(content: &[u8]) -> Vec<(start, end)>:
    if content.len() < 2:
        return []

    weights = pair_weights(content)
    out = []

    for left in 0..weights.len():
        max_inside = 0

        for right in left..weights.len():
            # Interval [left, right] in pair-space corresponds to bytes [left, right + 2)
            if right >= left + 2:
                max_inside = max(max_inside, weights[right - 1])

            if right <= left + 1:
                out.push((left, right + 2))
            else if weights[left] > max_inside and weights[right] > max_inside:
                out.push((left, right + 2))

    return out
```

The first implementation can be `O(n^2)` per file. We can optimize later with a monotone-stack or Cartesian-tree formulation, but the spec should define the semantics before the optimization.

#### `build_covering`

At query time we do **not** emit all sparse n-grams for a literal. Instead, we compute a minimum-cardinality cover of the literal's pair positions using sparse n-grams.

The first implementation uses the standard interval-cover greedy:

- Let `next_uncovered` be the leftmost uncovered pair position.
- Consider every sparse n-gram whose left edge is `<= next_uncovered`.
- Pick the candidate that reaches the farthest right.
- Break ties by choosing the earliest left edge.
- Advance `next_uncovered` to one past that interval's right edge and repeat.

This is the correct greedy for minimum interval cover on a line. Importantly, the chosen interval does **not** need to start at `next_uncovered`; it only needs to cover it.

```
function widest_from(weights: &[u32], left: usize) -> usize:
    max_inside = 0
    best = left

    for right in left..weights.len():
        if right >= left + 2:
            max_inside = max(max_inside, weights[right - 1])

        if right <= left + 1:
            best = right
        else if weights[left] > max_inside and weights[right] > max_inside:
            best = right

    return best   # largest valid right endpoint starting at `left`

function build_covering(content: &[u8]) -> Vec<(start, end)>:
    if content.len() < 2:
        return []

    weights = pair_weights(content)
    out = []
    frontier = 0
    next_uncovered = 0

    while next_uncovered < weights.len():
        best = None

        while frontier <= next_uncovered and frontier < weights.len():
            right = widest_from(weights, frontier)
            candidate = (frontier, right + 2)
            best = better_cover(best, candidate)  # farthest right, then earliest left
            frontier += 1

        out.push(best)
        next_uncovered = best.end - 1

    return out
```

This definition is deterministic because `widest_from(left)` is defined as the maximum valid right endpoint for that exact `left`, and `better_cover` makes tie-breaking explicit.

For recall, we only require two invariants:

1. every interval emitted by `build_covering` is a valid sparse n-gram and therefore also appears in `build_all`
2. the emitted intervals cover every pair position in the literal

Minimum-cardinality is now part of the intended behavior, but the first implementation should still validate it on short literals by brute force in tests.

**Recall property**: for any literal `L`, every n-gram emitted by `build_covering(L)` is also emitted by `build_all(L)`. Therefore, if a document contains `L`, that document contains every query n-gram in the covering set. Intersecting those posting lists cannot miss the document; it only narrows the candidate set before full regex verification.

N-gram extraction operates on **raw bytes**, not Unicode scalar values. This matches ripgrep's byte-oriented approach and avoids multi-byte complexity.

### Posting Lists

Each n-gram maps to a sorted list of file IDs. Stored sequentially in `postings.bin`, referenced by offset from `lookup.bin`. File IDs are delta-encoded as variable-length integers (varint) for compression.

### Lookup Table

Mmap'd hash table in `lookup.bin` using **linear probing** with a load factor of 0.7. Each slot: `(n-gram hash: u64, offset: u64, length: u32)` = 20 bytes. Empty slots indicated by `hash = 0`.

**Lookup procedure**: Compute slot = `hash % table_size`, compare stored hash. On mismatch, advance to next slot (linear probe). Stop at empty slot (hash = 0) — n-gram not in index. Hash collisions (different n-grams, same hash) are safe — they only widen the candidate set by merging posting lists, never miss results.

### File Table

Maps file ID (u32) to file path + metadata (mtime, size). Used to resolve candidates back to actual files.

### Git-aware State

- `meta.json` stores the base git commit hash for the current generation.
- The on-disk index is split into an immutable **base snapshot** plus a mutable **overlay** for dirty tracked files, untracked files, and deletions.
- `rsgrep update` refreshes only the overlay. Search loads base + overlay and suppresses any base file IDs listed in `overlay/tombstones.bin`.
- **For git repos**: the base snapshot corresponds to one commit; the overlay is derived from `git diff`, `git ls-files --others --exclude-standard`, and file deletions relative to that base.
- **For non-git directories**: the base snapshot is just the previous full index; `update` computes an overlay by comparing mtime/size/content hash against `files.bin`.
- File IDs are monotonically increasing across base + overlay. Deleted IDs are tombstoned, not reused.
- Periodic compaction (`rsgrep index --force`) merges the overlay into a fresh base generation and atomically switches `CURRENT`.

## Search / Query Pipeline

1. **Parse pattern**: Parse regex using `regex-syntax` crate to get an `Hir` (high-level intermediate representation).
2. **Extract mandatory literals**: Walk the `Hir` to extract byte literals that must appear in any match. For alternations (`a|b`), keep branch-local literals. For concatenations (`ab`), keep the combined literal when adjacent literals join exactly. For repetitions and wildcards, keep only literals that remain mandatory.
3. **Run `build_covering` on each usable literal**: For every extracted literal of length `>= 2`, compute its sparse covering set using the same byte-pair weight table as indexing. Mandatory literals shorter than 2 bytes are kept for regex verification but do not contribute index terms.
4. **Build query plan**: Literals decompose to `AND` over their covering n-grams. Alternations become `OR` over branch-local plans. Concatenations with multiple mandatory literals become `AND` over the literals' plans. Empty covering sets contribute no index constraint.
5. **Lookup posting lists**: For each query n-gram, hash it, probe `lookup.bin`, read the posting list from `postings.bin`.
6. **Merge base + overlay candidates**: Execute the query plan against base and overlay (AND = sorted intersection, OR = sorted union), union the results, then discard any tombstoned base file IDs.
7. **Verify with regex**: Read each candidate file and run the full regex in parallel. Only confirmed matches are emitted.
8. **Output**: Print in ripgrep-style format with colors, line numbers, and optional context.

### Fallback

If the regex yields no usable covering n-grams after decomposition, rsgrep falls back to scanning all indexed files from the merged base + overlay file tables. This includes patterns with no mandatory literals and patterns whose only mandatory literals are shorter than 2 bytes. It is still faster than directory traversal because the index already owns the candidate file set.

## Performance & Scalability

### Mmap Strategy

Both `lookup.bin` and `postings.bin` are memory-mapped. The OS manages page loading — small repos fit in RAM, large repos page efficiently from disk.

### Posting List Compression

Delta-encoded varint. For posting list `[10, 15, 23, 100]`, store `[10, 5, 8, 77]`. Significant disk/memory savings for large repos.

### Parallelism

- **Indexing**: `rayon` for parallel file reading and n-gram extraction.
- **Verification**: Candidate files verified against the regex in parallel.

### Incremental Updates

`rsgrep update` refreshes only the mutable overlay. `rsgrep index --force` compacts overlay state into a fresh immutable base generation.

### Ignore Rules

Respect `.gitignore` and `.rsgrep-ignore`. Skip binary files via null-byte sniffing (first 8KB).

### Expected Performance

| Operation | Small repo (~50K files) | Large repo (~500K files) |
|-----------|------------------------|-------------------------|
| Index build | 1-5 seconds | 30-60 seconds |
| Selective search (warm cache) | <100ms | <100ms |
| Broad search (warm cache) | <500ms | <500ms |
| Fallback (no n-grams) | ~ripgrep speed | ~ripgrep speed |

## Crate Architecture

```
rsgrep/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point (clap)
│   ├── lib.rs               # Public library API
│   ├── cli.rs               # Argument parsing & command dispatch
│   ├── index/
│   │   ├── mod.rs            # Index builder orchestration
│   │   ├── ngram.rs          # Sparse n-gram extraction + weight table
│   │   ├── postings.rs       # Posting list storage + delta encoding
│   │   ├── lookup.rs         # Hash table construction + mmap read
│   │   ├── filetable.rs      # File ID <-> path mapping
│   │   └── meta.rs           # Index metadata
│   ├── search/
│   │   ├── mod.rs            # Search pipeline orchestration
│   │   ├── decompose.rs      # Regex -> n-gram covering set
│   │   ├── intersect.rs      # Posting list intersection
│   │   └── verify.rs         # Candidate file regex verification
│   ├── output/
│   │   ├── mod.rs            # Output formatting
│   │   └── color.rs          # Terminal color support
│   └── ignore.rs             # .gitignore / .rsgrep-ignore handling
```

### Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI parsing |
| `regex` + `regex-syntax` | Regex matching and AST decomposition |
| `memmap2` | Memory-mapped file access |
| `rayon` | Parallel indexing and verification |
| `ignore` | Gitignore handling (same as ripgrep) |
| `anyhow` | Error handling |
| `serde` + `serde_json` | meta.json serialization |

## Error Handling & Edge Cases

| Scenario | Behavior |
|----------|----------|
| No index exists | Print message suggesting `rsgrep index`, exit non-zero |
| Stale index (>24h) | Print warning, still search |
| Corrupt index | Validate magic bytes, suggest `rsgrep index --force` |
| Binary files | Skip during indexing (null-byte sniffing, first 8KB) |
| Huge files (>10MB) | Skip by default, configurable via `--max-filesize` |
| Pure wildcard regex | Fall back to scanning all indexed files with a note |
| Symlinks | Follow by default, track inodes to avoid loops |
| Encoding | Treat as bytes like ripgrep, replace invalid UTF-8 on display |
| Concurrent access | Writers build a new generation directory, fsync it, then atomically switch `CURRENT`; `.rsgrep/lock` serializes writers |

## Testing Strategy

### Unit Tests

- N-gram extraction: weight function produces expected n-grams for known inputs
- Posting list encoding/decoding: roundtrip delta-varint
- Lookup table: insert and retrieve hashes correctly
- Regex decomposition: covering set for various patterns
- Posting list intersection: overlapping, disjoint, empty lists
- Sparse interval predicates: valid / invalid edge-dominance cases
- `build_covering`: greedy cover is minimal and every emitted n-gram is valid
- Equal-weight behavior: longer intervals with tied edge/interior weights are rejected
- Short literals: 1-byte literals yield no covering grams; 2-byte literals yield exactly one

### Integration Tests

- Build index on temp directory, search, verify results
- Incremental update: modify file, update index, verify
- Fallback: non-selective pattern returns all matches
- Binary/large file skipping
- Overlay semantics: dirty tracked file shadows base snapshot
- Untracked file appears through overlay without rebuilding base

### Property Tests

- For any pattern and file set, rsgrep results must be a superset of brute-force grep (never miss a match)
- **`build_all` / `build_covering` invariant**: for any literal `L`, every interval emitted by `build_covering(L)` must also be emitted by `build_all(L)`
- **Coverage invariant**: `build_covering(L)` must cover every pair position in `L`
- **Greedy optimality check**: for short random literals, greedy `build_covering` matches the minimum-cardinality cover found by brute force
- Use `proptest` to generate random contents and patterns

### Benchmarks

- `criterion` benchmarks for index build, search, and update
- Synthetic repos: 1K, 10K, 100K files
