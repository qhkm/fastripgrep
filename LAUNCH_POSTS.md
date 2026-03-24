# frg Launch Posts

**Product:** frg (fastripgrep) — grep alternative that beats ripgrep using sparse n-gram indexing
**URL:** https://github.com/qhkm/fastripgrep
**Install:** `cargo install fastripgrep` or `curl -fsSL https://raw.githubusercontent.com/qhkm/fastripgrep/main/install.sh | sh`
**Date:** 2026-03-24

---

## Hacker News (Show HN)

**Title:** Show HN: frg - A grep that beats ripgrep by indexing sparse n-grams

---

I built frg, a code search tool that pre-builds a sparse n-gram index and uses it to skip 99% of files before running the regex. It's faster than ripgrep on every pattern type I've tested.

**The idea:**

Ripgrep is fast because of SIMD regex, but it still reads every file on every search. On a 9K file codebase, that's 50-100ms per query no matter how selective your pattern is.

frg builds an index of "sparse n-grams" — variable-length byte substrings whose edge pair-weights strictly dominate all interior pair-weights. This is the approach described in Cursor's blog post about their code search (https://cursor.com/blog/fast-regex-search). At search time, frg decomposes your regex into a covering set of n-grams, intersects their posting lists, and only reads the handful of candidate files.

**Benchmarks (9K file codebase, warm cache):**

    Pattern              ripgrep    frg
    renderUsage          53ms       10ms (5.3x)
    function (24K hits)  75ms       51ms (1.5x)
    .*  (wildcard)       197ms      74ms (2.7x)
    useState|useEffect   54ms       13ms (4.2x)

Match counts are identical to ripgrep (case-sensitive) on all 7 patterns tested.

**How the index works:**

- 256x256 byte-pair weight table (corpus-derived, common pairs like "th" get low weights, rare pairs get high)
- `build_all` enumerates all valid sparse n-grams per file in O(n) with a 128-byte cap
- Delta-varint compressed posting lists + mmap'd linear-probing hash table
- Incremental updates via overlay/tombstone architecture (~70ms vs 20s full rebuild)

**For ScanAll patterns (`.*`, single char):**

frg can't use the index, but it still wins by: (1) skipping regex entirely for `.*`, (2) using memchr (SIMD) for single-byte patterns, (3) parallel file processing with rayon, (4) zero-copy streaming output.

**Other features:** `frg watch` (auto-update on file changes), `frg replace` (search & replace with dry-run preview), `frg init` (project detection), shell completions, smart-case, config file, self-upgrade.

97 tests, clippy clean, MIT licensed.

https://github.com/qhkm/fastripgrep

Happy to discuss the n-gram extraction algorithm, the covering set greedy, or any of the performance work.

---

## Reddit: r/rust

**Title:** I built frg — a grep that beats ripgrep on every pattern type using sparse n-gram indexing

---

Hey r/rust,

I built **frg** (fastripgrep), a code search tool in Rust that pre-builds a sparse n-gram index to skip 99% of files before running the regex.

**The result:** frg is 1.5-5.3x faster than ripgrep across all 8 pattern types I tested, with 100% match count parity.

### How it works

Instead of scanning every file like ripgrep, frg builds an index once (~20s for 9K files) and uses it to narrow candidates:

1. Extract "sparse n-grams" — variable-length byte substrings where both edge pair-weights strictly dominate all interior weights
2. Store in a mmap'd hash table with delta-varint compressed posting lists
3. At search time, decompose the regex into a covering set of n-grams, intersect posting lists, verify ~5 files instead of 9,000

The approach comes from [Cursor's blog post](https://cursor.com/blog/fast-regex-search) on fast regex search.

### Benchmarks (openclaw, 9K files)

| Pattern | ripgrep | frg | Speedup |
|---------|---------|-----|---------|
| `renderUsage` | 53ms | **10ms** | 5.3x |
| `.*` (wildcard) | 197ms | **74ms** | 2.7x |
| `function` (24K hits) | 75ms | **51ms** | 1.5x |

### Key crates used

- `regex` + `regex-syntax` for matching and AST decomposition
- `memmap2` for mmap'd index files
- `rayon` for parallel indexing and verification
- `memchr` for SIMD single-byte search
- `ignore` (same as ripgrep) for gitignore handling
- `xxhash-rust` for stable n-gram hashing
- `indicatif` for progress bars
- `notify` for file watching

### Features

- Incremental updates (~70ms overlay vs 20s full rebuild)
- `frg watch` — auto-update on file changes
- `frg replace` — search & replace with diff preview
- `frg init` — auto-detect project type
- Shell completions, smart-case, config file, self-upgrade

97 tests including property tests (proptest) for n-gram invariants.

**Install:** `cargo install fastripgrep`

**GitHub:** https://github.com/qhkm/fastripgrep

Would love feedback on the implementation, especially the n-gram extraction and covering algorithms.

---

## Reddit: r/commandline

**Title:** frg — a faster grep with indexing (beats ripgrep on every pattern type)

---

Built a grep alternative called **frg** that pre-builds an index for your codebase.

**The pitch:** First search takes ~20s to build the index. Every search after that is 10-70ms — even on 9K+ file codebases.

```
$ frg index .
Done. 8995 files, 5M n-grams.

$ time frg search "renderUsage" .
src/ui/app.ts:5:import { renderUsage } from "./views/usage.ts";
...
0.010s

$ time rg "renderUsage" .
...
0.053s
```

**All the flags you expect:**

```
-i          Case-insensitive
-S          Smart case (like rg)
-F          Fixed string
-l          Files only
-c          Count
-C 3        Context lines
-e pat      Multi-pattern
--type ts   File type filter
--glob      Glob filter
--json      JSON output
--no-index  Brute force (skip index)
```

**Extra stuff ripgrep doesn't have:**

- `frg watch` — watches for file changes, auto-updates index
- `frg replace "old" "new"` — search & replace with diff preview
- `frg init` — detects project type, creates ignore file
- `frg upgrade` — self-updates from GitHub releases

**Install:**

```
curl -fsSL https://raw.githubusercontent.com/qhkm/fastripgrep/main/install.sh | sh
```

or `cargo install fastripgrep`

https://github.com/qhkm/fastripgrep

---

## Reddit: r/SideProject

**Title:** I read Cursor's blog post about fast regex search and built it. It beats ripgrep on every pattern type.

---

Hey everyone,

Cursor published a blog post about how they built fast code search using "sparse n-gram indexing." I read it and thought: this should be a standalone CLI tool, not locked inside an IDE.

So I built **frg** (fastripgrep) — a grep alternative in Rust that indexes your codebase and uses it to skip 99% of files when searching.

### The Problem

ripgrep is amazing, but it reads every file on every search. On large codebases (9K+ files), even with SIMD regex, that's 50-100ms per query. When you're searching for a specific function name, reading 9,000 files to find it in 2 of them feels wasteful.

### The Solution

Build an index once (~20s), then search in 10-50ms by only reading the files that actually contain your pattern.

The index uses "sparse n-grams" — variable-length byte substrings selected by a weight function. Common byte pairs (like `th`, `er`) get low weights, rare pairs get high weights. This makes the n-grams naturally selective: rare character combinations produce longer, more unique index entries.

### Results

| Pattern | ripgrep | frg | Winner |
|---------|---------|-----|--------|
| Specific function | 53ms | 10ms | frg 5.3x |
| Broad pattern (24K hits) | 75ms | 51ms | frg 1.5x |
| Wildcard `.*` | 197ms | 74ms | frg 2.7x |

100% match count parity — frg never misses a result.

### Tech Stack

- Rust (single binary, ~5MB)
- 97 tests including property tests
- MIT licensed

### What I Learned

1. **The index is the easy part.** Getting match parity with ripgrep was the hard part — symlink behavior, binary detection, anchor semantics, all the edge cases.
2. **Streaming output matters.** Collecting 1.6M Match structs into a Vec before printing was 3x slower than streaming directly.
3. **memchr is magic.** For single-byte patterns, using SIMD memchr instead of regex made frg faster than ripgrep even without the index.

### Links

- GitHub: https://github.com/qhkm/fastripgrep
- Install: `cargo install fastripgrep`

Happy to answer questions about the implementation or the journey!

---

## Twitter/X Thread

**Tweet 1 (Hook):**

ripgrep reads every file on every search.

I built frg — a grep that indexes your codebase and skips 99% of files.

Result: 5x faster on real codebases.

https://github.com/qhkm/fastripgrep

Here's how it works:

---

**Tweet 2:**

The trick: sparse n-gram indexing.

Instead of scanning 9,000 files, frg:
1. Builds an index once (20s)
2. Decomposes your regex into n-gram lookups
3. Intersects posting lists
4. Reads only ~5 candidate files

10ms vs ripgrep's 53ms.

---

**Tweet 3:**

The benchmarks (9K file codebase):

renderUsage: 10ms vs 53ms (5.3x)
function: 51ms vs 75ms (1.5x)
.*: 74ms vs 197ms (2.7x)

frg wins on ALL 8 pattern types.
100% match count parity with ripgrep.

---

**Tweet 4:**

Even when the index can't help (wildcards, single chars), frg still wins by:

- Skipping regex entirely for .*
- SIMD memchr for single bytes
- Parallel file I/O with rayon
- Zero-copy streaming output

---

**Tweet 5:**

Built-in features ripgrep doesn't have:

- frg watch (auto-update index on save)
- frg replace (search & replace with diff preview)
- frg init (project detection)
- frg upgrade (self-update)
- Incremental updates (~70ms vs 20s rebuild)

---

**Tweet 6:**

Install:

cargo install fastripgrep

or:

curl -fsSL https://raw.githubusercontent.com/qhkm/fastripgrep/main/install.sh | sh

97 tests. MIT licensed. Built with Rust.

---

## LinkedIn

**Title:** I built a grep alternative that's faster than ripgrep — here's the engineering behind it

---

I just shipped frg (fastripgrep) — a code search tool that beats ripgrep on every pattern type by using sparse n-gram indexing.

The core insight: ripgrep (and grep) scan every file on every search. On a 9,000-file codebase, that's 50-100ms per query. frg builds an index once and uses it to read only the ~5 files that matter. Result: 10ms searches.

The approach comes from a technique Cursor uses in their IDE, described in their blog post "Fast Regex Search." I implemented it as a standalone Rust CLI tool.

Key engineering decisions:
- Corpus-derived byte-pair weights for n-gram selectivity (36% fewer index entries)
- Overlay/tombstone architecture for incremental updates (~70ms vs 20s full rebuild)
- Parallel streaming output with rayon for wildcard patterns
- SIMD byte search via memchr for single-character patterns

The result: 97 tests, 100% match parity with ripgrep, and faster performance across all 8 pattern types tested.

Open source (MIT): https://github.com/qhkm/fastripgrep

---

## Post Schedule

| Day | Platform | Time (MYT) |
|-----|----------|------------|
| Day 1 (Tue) | Hacker News | 9pm |
| Day 1 (Tue) | Twitter/X | 10pm |
| Day 2 (Wed) | r/rust | 10pm |
| Day 2 (Wed) | LinkedIn | 9pm |
| Day 3 (Thu) | r/commandline | 10pm |
| Day 4 (Fri) | r/SideProject | 10pm |

---

## FAQ Prep

**Q: Why not just use ripgrep?**
A: ripgrep is great for one-off searches. frg is better when you search the same codebase repeatedly — the index amortizes over many queries.

**Q: 20s to build the index is slow.**
A: It's a one-time cost. After that, `frg update` takes ~70ms (incremental). And `frg watch` auto-updates on file changes.

**Q: Does it work on large monorepos?**
A: Tested on 9K files. The O(n) n-gram extraction with 128-byte cap and parallel processing with rayon should scale well. Haven't tested on 100K+ yet — feedback welcome.

**Q: Why not use trigrams like codesearch?**
A: Trigrams have massive posting lists for common 3-byte sequences. Sparse n-grams are variable-length and weighted by frequency, producing much more selective index entries (5M vs ~20M for trigrams on the same codebase).

**Q: Is match parity really 100%?**
A: Tested against ripgrep (case-sensitive mode) on 7 different patterns. All returned identical match counts. The only difference was ripgrep's `--smart-case` config, which is a user setting, not a tool difference.

**Q: What about memory usage?**
A: Index files are mmap'd, so the OS manages paging. The index for 9K files is ~80MB on disk. No resident memory overhead beyond the mmap'd pages the OS decides to keep warm.

---

## Short Copy

**One-liner:** frg — grep that beats ripgrep using sparse n-gram indexing

**Elevator pitch:** frg pre-indexes your codebase so searches read 5 files instead of 9,000. 5x faster than ripgrep on selective patterns, with 100% match parity.

**Tweet-length:** Built a grep that beats ripgrep on every pattern type. Indexes your codebase with sparse n-grams, searches in 10ms instead of 50ms. Rust, MIT licensed. https://github.com/qhkm/fastripgrep
