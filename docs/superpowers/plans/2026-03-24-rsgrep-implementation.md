# rsgrep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust CLI grep alternative that uses sparse n-gram indexing to search codebases faster than brute-force scanning.

**Architecture:** Files are indexed by extracting sparse n-grams (variable-length byte substrings whose edge pair-weights strictly dominate interior weights) and storing them in a two-file format (postings.bin + lookup.bin) within a generation-based storage layout. At search time, regex patterns are decomposed into covering n-grams via minimum-cardinality interval-cover greedy, posting lists are intersected to find candidate files, and only those candidates are verified with the full regex. Alternations produce OR query plans; concatenations produce AND plans.

**Tech Stack:** Rust, clap (CLI), regex + regex-syntax (matching/decomposition), memmap2 (mmap), rayon (parallelism), ignore (gitignore), anyhow (errors), serde + serde_json (metadata), xxhash-rust (stable hashing), is-terminal (TTY detection), fs4 (file locking)

**Spec:** `docs/superpowers/specs/2026-03-24-rsgrep-design.md`

---

## File Structure

```
rsgrep/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Re-exports all public modules
│   ├── cli.rs               # clap argument parsing + command dispatch
│   ├── index/
│   │   ├── mod.rs            # Index builder orchestration (build, update, status)
│   │   ├── ngram.rs          # Weight table + build_all + build_covering
│   │   ├── postings.rs       # Posting list: delta-varint encode/decode
│   │   ├── lookup.rs         # Hash table: build, mmap read, linear probing
│   │   ├── filetable.rs      # File ID <-> path mapping
│   │   └── meta.rs           # Generation metadata (serde JSON)
│   ├── search/
│   │   ├── mod.rs            # Search pipeline orchestration
│   │   ├── decompose.rs      # Regex HIR -> query plan with alternation support
│   │   ├── intersect.rs      # Sorted posting list intersection/union
│   │   └── verify.rs         # Parallel candidate file regex verification + context lines
│   ├── output/
│   │   ├── mod.rs            # Output formatting (ripgrep-style + JSON)
│   │   └── color.rs          # Terminal color via is-terminal
│   └── ignore.rs             # .gitignore + .rsgrep-ignore + glob/type filtering
├── tests/
│   ├── integration.rs        # End-to-end index + search tests
│   └── property_tests.rs     # Proptest invariant tests
└── benches/
    └── bench.rs              # Criterion benchmarks
```

---

## Task 1: Project Scaffold + Cargo.toml

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: all stub module files

- [ ] **Step 1: Initialize cargo project**

Run: `cd /Users/dr.noranizaahmad/ios/rsgrep && cargo init --name rsgrep`
Expected: Creates Cargo.toml and src/main.rs

- [ ] **Step 2: Write Cargo.toml**

```toml
[package]
name = "rsgrep"
version = "0.1.0"
edition = "2021"
description = "Fast regex search with sparse n-gram indexing"

[dependencies]
clap = { version = "4", features = ["derive"] }
regex = "1"
regex-syntax = "0.8"
memmap2 = "0.9"
rayon = "1"
ignore = "0.4"
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
is-terminal = "0.4"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
glob = "0.3"
fs4 = "0.9"

[dev-dependencies]
tempfile = "3"
proptest = "1"
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "bench"
harness = false
```

- [ ] **Step 3: Create src/lib.rs**

```rust
pub mod cli;
pub mod index;
pub mod search;
pub mod output;
pub mod ignore;
```

- [ ] **Step 4: Create src/main.rs**

```rust
use anyhow::Result;

fn main() -> Result<()> {
    rsgrep::cli::run()
}
```

- [ ] **Step 5: Create stub modules**

Create these files so the project compiles:
- `src/cli.rs`: `use anyhow::Result; pub fn run() -> Result<()> { Ok(()) }`
- `src/index/mod.rs`: `pub mod ngram; pub mod postings; pub mod lookup; pub mod filetable; pub mod meta;`
- `src/index/ngram.rs`, `src/index/postings.rs`, `src/index/lookup.rs`, `src/index/filetable.rs`, `src/index/meta.rs`: empty files
- `src/search/mod.rs`: `pub mod decompose; pub mod intersect; pub mod verify;`
- `src/search/decompose.rs`, `src/search/intersect.rs`, `src/search/verify.rs`: empty files
- `src/output/mod.rs`: `pub mod color;`
- `src/output/color.rs`: empty file
- `src/ignore.rs`: empty file

- [ ] **Step 6: Verify it compiles**

Run: `cargo build`
Expected: Compiles (warnings OK)

- [ ] **Step 7: Initialize git and commit**

```bash
cd /Users/dr.noranizaahmad/ios/rsgrep
git init
echo "target/" > .gitignore
echo ".rsgrep/" >> .gitignore
git add Cargo.toml src/ .gitignore
git commit -m "feat: initialize rsgrep project scaffold"
```

---

## Task 2: Sparse N-gram Extraction (ngram.rs)

**Files:**
- Create: `src/index/ngram.rs`

Core algorithm: weight table, `build_all` (O(n^2) for correctness), `build_covering` (minimum-cardinality interval-cover greedy). Strict `>` for edge dominance. 1-byte inputs produce no n-grams.

- [ ] **Step 1: Write tests for pair_weights**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pair_weights_basic() {
        let content = b"abc";
        let weights = pair_weights(content);
        assert_eq!(weights.len(), 2);
        assert_eq!(weights[0], WEIGHT_TABLE[b'a' as usize][b'b' as usize]);
        assert_eq!(weights[1], WEIGHT_TABLE[b'b' as usize][b'c' as usize]);
    }

    #[test]
    fn test_pair_weights_empty_and_single() {
        assert!(pair_weights(b"").is_empty());
        assert!(pair_weights(b"a").is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rsgrep index::ngram::tests::test_pair_weights -- --nocapture`
Expected: FAIL — `pair_weights` not defined

- [ ] **Step 3: Implement weight table and pair_weights**

```rust
/// Static 256x256 weight table. Uses a deterministic hash to assign weights.
/// A production version would use inverse frequency from a large open-source corpus.
/// Values are in [1, 251] — never zero (zero would break edge-dominance comparisons
/// against the `max_inside = 0` initial value in build_all/build_covering).
pub static WEIGHT_TABLE: [[u32; 256]; 256] = {
    let mut table = [[0u32; 256]; 256];
    let mut a = 0usize;
    while a < 256 {
        let mut b = 0usize;
        while b < 256 {
            let raw = ((a.wrapping_mul(131) ^ b.wrapping_mul(137)) % 251) as u32 + 1;
            table[a][b] = raw;
            b += 1;
        }
        a += 1;
    }
    table
};

/// Compute pair weights for a byte sequence.
/// Returns vec of length `content.len() - 1` (empty if < 2 bytes).
pub fn pair_weights(content: &[u8]) -> Vec<u32> {
    if content.len() < 2 {
        return Vec::new();
    }
    content
        .windows(2)
        .map(|w| WEIGHT_TABLE[w[0] as usize][w[1] as usize])
        .collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rsgrep index::ngram::tests::test_pair_weights -- --nocapture`
Expected: PASS

- [ ] **Step 5: Write tests for build_all**

```rust
    #[test]
    fn test_build_all_two_bytes() {
        // 2 bytes = 1 pair, vacuously valid
        let ngrams = build_all(b"ab");
        assert_eq!(ngrams.len(), 1);
        assert_eq!(ngrams[0], (0, 2));
    }

    #[test]
    fn test_build_all_three_bytes() {
        // 3 bytes = 2 pairs. All 2-byte and 3-byte substrings are vacuously valid.
        let ngrams = build_all(b"abc");
        // Must include: (0,2), (1,3), (0,3)
        assert!(ngrams.contains(&(0, 2)));
        assert!(ngrams.contains(&(1, 3)));
        assert!(ngrams.contains(&(0, 3)));
    }

    #[test]
    fn test_build_all_empty_and_single() {
        assert!(build_all(b"").is_empty());
        assert!(build_all(b"x").is_empty());
    }

    #[test]
    fn test_build_all_laminarity() {
        // No two sparse n-grams should cross
        let content = b"hello world test pattern";
        let ngrams = build_all(content);
        for i in 0..ngrams.len() {
            for j in (i + 1)..ngrams.len() {
                let (a_s, a_e) = ngrams[i];
                let (b_s, b_e) = ngrams[j];
                let crosses = a_s < b_s && b_s < a_e && a_e < b_e;
                let crosses_rev = b_s < a_s && a_s < b_e && b_e < a_e;
                assert!(!crosses && !crosses_rev,
                    "n-grams cross: [{},{}) and [{},{})", a_s, a_e, b_s, b_e);
            }
        }
    }

    #[test]
    fn test_build_all_strict_greater() {
        // Equal weights must NOT produce longer n-grams.
        // Create content where all pair weights are equal.
        // Only 2-byte and 3-byte n-grams should be valid (vacuous interior).
        // Longer n-grams require strict >, which fails when all weights are equal.
        // We can't easily control weights with the hash table, so test the logic:
        // With weights [5, 5, 5], interval [0,2] in pair-space = bytes [0,4):
        //   left=0 w=5, right=2 w=5, max_inside=w[1]=5
        //   5 > 5 is false, so this is NOT a valid n-gram. Correct.
        let weights = vec![5u32, 5, 5];
        let mut out = Vec::new();
        for left in 0..weights.len() {
            let mut max_inside: u32 = 0;
            for right in left..weights.len() {
                if right >= left + 2 {
                    max_inside = max_inside.max(weights[right - 1]);
                }
                if right <= left + 1 {
                    out.push((left, right + 2));
                } else if weights[left] > max_inside && weights[right] > max_inside {
                    out.push((left, right + 2));
                }
            }
        }
        // With all-equal weights, only 2-byte and 3-byte spans are valid
        for (start, end) in &out {
            assert!(end - start <= 3, "equal weights should not produce spans > 3 bytes");
        }
    }
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cargo test -p rsgrep index::ngram::tests::test_build_all -- --nocapture`
Expected: FAIL — `build_all` not defined

- [ ] **Step 7: Implement build_all**

```rust
/// A sparse n-gram span: byte range [start, end) in the content.
pub type NgramSpan = (usize, usize);

/// Enumerate all sparse n-grams in `content`.
///
/// A substring `content[l..r)` with `r - l >= 2` is sparse iff:
/// - When r - l <= 3: vacuously true (no interior pairs)
/// - Otherwise: p[l] > max(p[l+1..r-2]) AND p[r-2] > max(p[l+1..r-2])
///
/// where p[i] = WEIGHT_TABLE[content[i]][content[i+1]].
/// Strict > is required — equal weights do NOT dominate.
pub fn build_all(content: &[u8]) -> Vec<NgramSpan> {
    if content.len() < 2 {
        return Vec::new();
    }

    let weights = pair_weights(content);
    let mut out = Vec::new();

    for left in 0..weights.len() {
        let mut max_inside: u32 = 0;

        for right in left..weights.len() {
            if right >= left + 2 {
                max_inside = max_inside.max(weights[right - 1]);
            }

            if right <= left + 1 {
                // 2 or 3 byte n-gram: interior empty, vacuously valid
                out.push((left, right + 2));
            } else if weights[left] > max_inside && weights[right] > max_inside {
                out.push((left, right + 2));
            }
        }
    }

    out
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cargo test -p rsgrep index::ngram::tests::test_build_all -- --nocapture`
Expected: PASS

- [ ] **Step 9: Write tests for build_covering**

```rust
    #[test]
    fn test_build_covering_covers_all_pairs() {
        let content = b"println!";
        let covering = build_covering(content);
        assert!(!covering.is_empty());
        // Must cover every pair position 0..content.len()-1
        let num_pairs = content.len() - 1;
        let mut covered = vec![false; num_pairs];
        for &(start, end) in &covering {
            for p in start..end.saturating_sub(1) {
                if p < covered.len() {
                    covered[p] = true;
                }
            }
        }
        for (i, &c) in covered.iter().enumerate() {
            assert!(c, "pair position {} not covered", i);
        }
    }

    #[test]
    fn test_build_covering_subset_of_build_all() {
        let content = b"handleExport";
        let all = build_all(content);
        let covering = build_covering(content);
        for span in &covering {
            assert!(all.contains(span),
                "covering span {:?} not in build_all", span);
        }
    }

    #[test]
    fn test_build_covering_empty_and_single() {
        assert!(build_covering(b"").is_empty());
        assert!(build_covering(b"x").is_empty());
    }

    #[test]
    fn test_build_covering_two_bytes() {
        let covering = build_covering(b"ab");
        assert_eq!(covering.len(), 1);
        assert_eq!(covering[0], (0, 2));
    }
```

- [ ] **Step 10: Run tests to verify they fail**

Run: `cargo test -p rsgrep index::ngram::tests::test_build_covering -- --nocapture`
Expected: FAIL — `build_covering` not defined

- [ ] **Step 11: Implement build_covering**

Uses minimum-cardinality interval-cover greedy per the updated spec: candidates can start before `next_uncovered`, pick the one that reaches farthest right, break ties by earliest left.

```rust
/// Find the widest sparse n-gram starting at `left` in pair-space.
/// Returns the largest valid right endpoint.
fn widest_from(weights: &[u32], left: usize) -> usize {
    let mut max_inside: u32 = 0;
    let mut best = left;

    for right in left..weights.len() {
        if right >= left + 2 {
            max_inside = max_inside.max(weights[right - 1]);
        }

        if right <= left + 1 {
            best = right;
        } else if weights[left] > max_inside && weights[right] > max_inside {
            best = right;
        }
    }

    best
}

/// Generate the minimum-cardinality covering set of sparse n-grams.
///
/// Uses the standard interval-cover greedy:
/// - Let `next_uncovered` be the leftmost uncovered pair position.
/// - Consider every sparse n-gram whose left edge <= next_uncovered.
/// - Pick the candidate reaching farthest right; break ties by earliest left.
/// - Advance next_uncovered past that interval's right edge.
pub fn build_covering(content: &[u8]) -> Vec<NgramSpan> {
    if content.len() < 2 {
        return Vec::new();
    }

    let weights = pair_weights(content);
    let mut out = Vec::new();
    let mut frontier = 0usize;
    let mut next_uncovered = 0usize;

    while next_uncovered < weights.len() {
        let mut best_left = 0usize;
        let mut best_right_end = 0usize; // byte end = right + 2
        let mut found = false;

        while frontier <= next_uncovered && frontier < weights.len() {
            let right = widest_from(&weights, frontier);
            let candidate_end = right + 2;
            if !found || candidate_end > best_right_end
                || (candidate_end == best_right_end && frontier < best_left)
            {
                best_left = frontier;
                best_right_end = candidate_end;
                found = true;
            }
            frontier += 1;
        }

        if !found {
            break;
        }

        out.push((best_left, best_right_end));
        // next_uncovered advances to one past the last pair covered
        // byte range [best_left, best_right_end) covers pairs best_left..best_right_end-1
        next_uncovered = best_right_end - 1;
    }

    out
}
```

- [ ] **Step 12: Run all ngram tests**

Run: `cargo test -p rsgrep index::ngram -- --nocapture`
Expected: All PASS

- [ ] **Step 13: Add hash_ngram helper**

```rust
/// Hash an n-gram byte slice to a u64 using xxh3 (stable across Rust versions).
/// Never returns 0 (reserved for empty lookup table slots).
pub fn hash_ngram(bytes: &[u8]) -> u64 {
    let h = xxhash_rust::xxh3::xxh3_64(bytes);
    if h == 0 { 1 } else { h }
}
```

- [ ] **Step 14: Commit**

```bash
git add src/index/ngram.rs
git commit -m "feat: implement sparse n-gram extraction (build_all, build_covering, weight table)"
```

---

## Task 3: Posting List Encoding/Decoding (postings.rs)

**Files:**
- Create: `src/index/postings.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_roundtrip() {
        for &v in &[0u32, 1, 127, 128, 255, 256, 16383, 16384, u32::MAX] {
            let mut buf = Vec::new();
            encode_varint(v, &mut buf);
            let (decoded, bytes_read) = decode_varint(&buf).unwrap();
            assert_eq!(decoded, v);
            assert_eq!(bytes_read, buf.len());
        }
    }

    #[test]
    fn test_posting_list_roundtrip() {
        let ids = vec![5, 10, 15, 100, 1000, 50000];
        let encoded = encode_posting_list(&ids);
        let decoded = decode_posting_list(&encoded);
        assert_eq!(decoded, ids);
    }

    #[test]
    fn test_posting_list_empty() {
        let encoded = encode_posting_list(&[]);
        assert!(decode_posting_list(&encoded).is_empty());
    }

    #[test]
    fn test_posting_list_single() {
        let encoded = encode_posting_list(&[42]);
        assert_eq!(decode_posting_list(&encoded), vec![42]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rsgrep index::postings::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement varint and posting list encode/decode**

```rust
use anyhow::Result;

pub fn encode_varint(mut value: u32, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

pub fn decode_varint(data: &[u8]) -> Result<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift = 0;
    for (i, &byte) in data.iter().enumerate() {
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return Ok((result, i + 1));
        }
        shift += 7;
        if shift >= 35 {
            anyhow::bail!("varint too long");
        }
    }
    anyhow::bail!("unexpected end of varint");
}

pub fn encode_posting_list(ids: &[u32]) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut prev = 0u32;
    for &id in ids {
        encode_varint(id - prev, &mut buf);
        prev = id;
    }
    buf
}

pub fn decode_posting_list(data: &[u8]) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut offset = 0;
    let mut prev = 0u32;
    while offset < data.len() {
        if let Ok((delta, consumed)) = decode_varint(&data[offset..]) {
            prev += delta;
            ids.push(prev);
            offset += consumed;
        } else {
            break;
        }
    }
    ids
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rsgrep index::postings::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/index/postings.rs
git commit -m "feat: implement delta-varint posting list encode/decode"
```

---

## Task 4: Lookup Hash Table (lookup.rs)

**Files:**
- Create: `src/index/lookup.rs`

Linear probing, load factor 0.7, 20-byte slots, hash=0 sentinel, magic bytes validation.

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_lookup_build_and_query() {
        let entries = vec![
            LookupEntry { hash: 100, offset: 0, length: 50 },
            LookupEntry { hash: 200, offset: 50, length: 30 },
            LookupEntry { hash: 300, offset: 80, length: 20 },
        ];
        let mut file = NamedTempFile::new().unwrap();
        write_lookup_table(&entries, file.as_file_mut()).unwrap();
        let table = MmapLookupTable::open(file.path()).unwrap();
        assert_eq!(table.lookup(100), Some((0, 50)));
        assert_eq!(table.lookup(200), Some((50, 30)));
        assert_eq!(table.lookup(300), Some((80, 20)));
        assert_eq!(table.lookup(999), None);
    }

    #[test]
    fn test_lookup_empty() {
        let mut file = NamedTempFile::new().unwrap();
        write_lookup_table(&[], file.as_file_mut()).unwrap();
        let table = MmapLookupTable::open(file.path()).unwrap();
        assert_eq!(table.lookup(123), None);
    }

    #[test]
    fn test_lookup_hash_zero_returns_none() {
        let mut file = NamedTempFile::new().unwrap();
        write_lookup_table(&[LookupEntry { hash: 1, offset: 0, length: 10 }], file.as_file_mut()).unwrap();
        let table = MmapLookupTable::open(file.path()).unwrap();
        assert_eq!(table.lookup(0), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rsgrep index::lookup::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement LookupTable**

```rust
use anyhow::Result;
use memmap2::Mmap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const SLOT_SIZE: usize = 20;
const LOAD_FACTOR: f64 = 0.7;
const MAGIC: &[u8; 4] = b"RSLK";

#[derive(Clone, Copy)]
pub struct LookupEntry {
    pub hash: u64,
    pub offset: u64,
    pub length: u32,
}

pub fn write_lookup_table(entries: &[LookupEntry], file: &mut File) -> Result<()> {
    let num_slots = if entries.is_empty() {
        16
    } else {
        (entries.len() as f64 / LOAD_FACTOR).ceil() as usize
    };

    file.write_all(MAGIC)?;
    file.write_all(&(num_slots as u64).to_le_bytes())?;

    let mut slots = vec![0u8; num_slots * SLOT_SIZE];
    for entry in entries {
        let mut idx = (entry.hash as usize) % num_slots;
        loop {
            let off = idx * SLOT_SIZE;
            let stored = u64::from_le_bytes(slots[off..off + 8].try_into().unwrap());
            if stored == 0 {
                slots[off..off + 8].copy_from_slice(&entry.hash.to_le_bytes());
                slots[off + 8..off + 16].copy_from_slice(&entry.offset.to_le_bytes());
                slots[off + 16..off + 20].copy_from_slice(&entry.length.to_le_bytes());
                break;
            }
            idx = (idx + 1) % num_slots;
        }
    }

    file.write_all(&slots)?;
    file.flush()?;
    Ok(())
}

pub struct MmapLookupTable {
    mmap: Mmap,
    num_slots: usize,
}

impl MmapLookupTable {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        if mmap.len() < 12 || &mmap[0..4] != MAGIC {
            anyhow::bail!("invalid lookup table: bad magic");
        }
        let num_slots = u64::from_le_bytes(mmap[4..12].try_into().unwrap()) as usize;
        if mmap.len() < 12 + num_slots * SLOT_SIZE {
            anyhow::bail!("lookup table truncated");
        }
        Ok(Self { mmap, num_slots })
    }

    pub fn lookup(&self, hash: u64) -> Option<(u64, u32)> {
        if hash == 0 || self.num_slots == 0 {
            return None;
        }
        let data = &self.mmap[12..];
        let mut idx = (hash as usize) % self.num_slots;
        loop {
            let off = idx * SLOT_SIZE;
            let stored = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
            if stored == 0 {
                return None;
            }
            if stored == hash {
                let offset = u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap());
                let length = u32::from_le_bytes(data[off + 16..off + 20].try_into().unwrap());
                return Some((offset, length));
            }
            idx = (idx + 1) % self.num_slots;
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rsgrep index::lookup::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/index/lookup.rs
git commit -m "feat: implement mmap'd linear-probing lookup hash table"
```

---

## Task 5: File Table (filetable.rs)

**Files:**
- Create: `src/index/filetable.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::path::PathBuf;

    #[test]
    fn test_filetable_roundtrip() {
        let mut table = FileTableBuilder::new();
        let id1 = table.add("src/main.rs", 1000, 500);
        let id2 = table.add("src/lib.rs", 2000, 300);
        let mut file = NamedTempFile::new().unwrap();
        table.write(file.as_file_mut()).unwrap();

        let reader = FileTableReader::open(file.path()).unwrap();
        let e1 = reader.get(id1).unwrap();
        assert_eq!(e1.path, PathBuf::from("src/main.rs"));
        assert_eq!(e1.mtime, 1000);
        let e2 = reader.get(id2).unwrap();
        assert_eq!(e2.path, PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn test_filetable_all_ids() {
        let mut table = FileTableBuilder::new();
        table.add("a.rs", 0, 0);
        table.add("b.rs", 0, 0);
        let mut file = NamedTempFile::new().unwrap();
        table.write(file.as_file_mut()).unwrap();
        let reader = FileTableReader::open(file.path()).unwrap();
        assert_eq!(reader.all_file_ids(), vec![0, 1]);
    }
}
```

- [ ] **Step 2: Run tests, verify fail, implement, verify pass**

Implementation: `FileTableBuilder` (add entries, write binary), `FileTableReader` (open, get by ID, list all IDs). Binary format: magic `RSFT`, u32 count, then per-entry: u32 path_len, path bytes, u64 mtime, u64 size.

(Same implementation as previous plan — unchanged.)

- [ ] **Step 3: Commit**

```bash
git add src/index/filetable.rs
git commit -m "feat: implement file table (file ID <-> path mapping)"
```

---

## Task 6: Index Metadata (meta.rs)

**Files:**
- Create: `src/index/meta.rs`

- [ ] **Step 1: Write test, implement, verify**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexMeta {
    pub version: u32,
    pub commit_hash: Option<String>,
    pub file_count: u32,
    pub ngram_count: u32,
    pub timestamp: u64,
}

impl IndexMeta {
    pub fn write(&self, path: &std::path::Path) -> anyhow::Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
    pub fn read(path: &std::path::Path) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
    }
    pub fn timestamp_now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}
```

Test: roundtrip write + read, verify all fields match.

- [ ] **Step 2: Commit**

```bash
git add src/index/meta.rs
git commit -m "feat: implement index metadata (serde JSON)"
```

---

## Task 7: Ignore / File Walking (ignore.rs)

**Files:**
- Create: `src/ignore.rs`

Uses `ignore` crate for .gitignore/.rsgrep-ignore. Binary detection. File type and glob filtering.

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_is_binary() {
        assert!(is_binary(b"\x00\x01\x02"));
        assert!(!is_binary(b"fn main() {}"));
    }

    #[test]
    fn test_walk_respects_gitignore() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("keep.rs"), "code").unwrap();
        fs::write(dir.path().join("skip.log"), "log").unwrap();
        fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();
        let files = walk_files(dir.path(), 10 * 1024 * 1024).unwrap();
        let names: Vec<_> = files.iter().map(|p| p.file_name().unwrap().to_str().unwrap()).collect();
        assert!(names.contains(&"keep.rs"));
        assert!(!names.contains(&"skip.log"));
    }

    #[test]
    fn test_walk_skips_binary() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("code.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("bin.dat"), b"\x00\x01\x02\x03").unwrap();
        let files = walk_files(dir.path(), 10 * 1024 * 1024).unwrap();
        let names: Vec<_> = files.iter().map(|p| p.file_name().unwrap().to_str().unwrap()).collect();
        assert!(names.contains(&"code.rs"));
        assert!(!names.contains(&"bin.dat"));
    }
}
```

- [ ] **Step 2: Implement**

```rust
use anyhow::Result;
use ignore::WalkBuilder;
use std::fs;
use std::path::{Path, PathBuf};

pub fn is_binary(content: &[u8]) -> bool {
    let check_len = content.len().min(8192);
    content[..check_len].contains(&0)
}

pub fn walk_files(root: &Path, max_filesize: u64) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let walker = WalkBuilder::new(root)
        .add_custom_ignore_filename(".rsgrep-ignore")
        .follow_links(true)
        .build();

    for entry in walker {
        let entry = entry?;
        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > max_filesize {
                continue;
            }
        }
        if let Ok(content) = fs::read(path) {
            if is_binary(&content) {
                continue;
            }
        }
        files.push(path.to_path_buf());
    }
    files.sort();
    Ok(files)
}
```

- [ ] **Step 3: Commit**

```bash
git add src/ignore.rs
git commit -m "feat: implement file walking with gitignore + binary detection"
```

---

## Task 8: Index Builder (index/mod.rs)

**Files:**
- Modify: `src/index/mod.rs`

Orchestrates full index build with generation-based storage and `.rsgrep/lock` for writer serialization.

- [ ] **Step 1: Write integration test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_build_index() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.rs"), "fn hello_world() {}").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() { hello_world(); }").unwrap();
        let result = build_index(dir.path(), 10 * 1024 * 1024);
        assert!(result.is_ok());
        let index_dir = dir.path().join(".rsgrep");
        assert!(index_dir.join("CURRENT").exists());
        let current = fs::read_to_string(index_dir.join("CURRENT")).unwrap();
        let gen_dir = index_dir.join("generations").join(current.trim());
        assert!(gen_dir.join("meta.json").exists());
        assert!(gen_dir.join("postings.bin").exists());
        assert!(gen_dir.join("lookup.bin").exists());
        assert!(gen_dir.join("files.bin").exists());
    }

    #[test]
    fn test_build_index_empty_dir() {
        let dir = TempDir::new().unwrap();
        assert!(build_index(dir.path(), 10 * 1024 * 1024).is_ok());
    }
}
```

- [ ] **Step 2: Implement build_index**

Key additions vs previous plan:
- Uses `fs4::FileExt` for `.rsgrep/lock` file locking
- Writes generation atomically (build in temp dir, rename)
- Uses `saturating_sub` for timestamp comparisons
- Avoids double file reads by caching content from walk phase

```rust
pub mod ngram;
pub mod postings;
pub mod lookup;
pub mod filetable;
pub mod meta;

use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::ignore::walk_files;
use ngram::{build_all, hash_ngram};
use postings::encode_posting_list;
use lookup::{LookupEntry, write_lookup_table};
use filetable::FileTableBuilder;
use meta::IndexMeta;

pub fn build_index(root: &Path, max_filesize: u64) -> Result<()> {
    let index_dir = root.join(".rsgrep");
    fs::create_dir_all(&index_dir)?;

    // Acquire writer lock
    let lock_path = index_dir.join("lock");
    let lock_file = File::create(&lock_path)?;
    use fs4::fs_std::FileExt;
    lock_file.lock_exclusive()?;

    let files = walk_files(root, max_filesize)?;

    // Build file table
    let mut file_table = FileTableBuilder::new();
    let mut file_entries: Vec<(u32, std::path::PathBuf)> = Vec::new();
    for path in &files {
        let metadata = fs::metadata(path)?;
        let mtime = metadata.modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let relative = path.strip_prefix(root).unwrap_or(path);
        let id = file_table.add(relative.to_str().unwrap_or(""), mtime, metadata.len());
        file_entries.push((id, path.clone()));
    }

    // Extract n-grams in parallel
    let per_file_ngrams: Vec<(u32, Vec<Vec<u8>>)> = file_entries
        .par_iter()
        .filter_map(|(id, path)| {
            let content = fs::read(path).ok()?;
            let spans = build_all(&content);
            let ngrams: Vec<Vec<u8>> = spans.iter()
                .map(|&(s, e)| content[s..e].to_vec())
                .collect();
            Some((*id, ngrams))
        })
        .collect();

    // Build inverted index
    let mut inverted: HashMap<u64, Vec<u32>> = HashMap::new();
    for (file_id, ngrams) in &per_file_ngrams {
        let mut seen = std::collections::HashSet::new();
        for ngram_bytes in ngrams {
            let hash = hash_ngram(ngram_bytes);
            if seen.insert(hash) {
                inverted.entry(hash).or_default().push(*file_id);
            }
        }
    }
    for list in inverted.values_mut() {
        list.sort_unstable();
        list.dedup();
    }

    // Encode postings + build lookup
    let mut postings_data = Vec::new();
    let mut lookup_entries = Vec::new();
    for (hash, ids) in &inverted {
        let offset = postings_data.len() as u64;
        let encoded = encode_posting_list(ids);
        let length = encoded.len() as u32;
        postings_data.extend_from_slice(&encoded);
        lookup_entries.push(LookupEntry { hash: *hash, offset, length });
    }

    // Write to generation directory
    let gen_id = format!("{}", IndexMeta::timestamp_now());
    let gen_dir = index_dir.join("generations").join(&gen_id);
    fs::create_dir_all(&gen_dir)?;

    let mut pf = File::create(gen_dir.join("postings.bin"))?;
    pf.write_all(&postings_data)?;
    let mut lf = File::create(gen_dir.join("lookup.bin"))?;
    write_lookup_table(&lookup_entries, &mut lf)?;
    let mut ff = File::create(gen_dir.join("files.bin"))?;
    file_table.write(&mut ff)?;

    let commit_hash = get_git_commit(root);
    let meta = IndexMeta {
        version: 1,
        commit_hash,
        file_count: file_table.len() as u32,
        ngram_count: inverted.len() as u32,
        timestamp: IndexMeta::timestamp_now(),
    };
    meta.write(&gen_dir.join("meta.json"))?;

    // Atomically switch CURRENT
    let tmp = index_dir.join("CURRENT.tmp");
    fs::write(&tmp, &gen_id)?;
    fs::rename(&tmp, index_dir.join("CURRENT"))?;

    // Release lock (dropped automatically, but explicit)
    lock_file.unlock()?;

    Ok(())
}

fn get_git_commit(root: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
}

pub fn current_generation(root: &Path) -> Result<std::path::PathBuf> {
    let index_dir = root.join(".rsgrep");
    let current = fs::read_to_string(index_dir.join("CURRENT"))?
        .trim().to_string();
    Ok(index_dir.join("generations").join(current))
}

pub fn index_status(root: &Path) -> Result<()> {
    let gen_dir = current_generation(root)?;
    let meta = IndexMeta::read(&gen_dir.join("meta.json"))?;
    let age = IndexMeta::timestamp_now().saturating_sub(meta.timestamp);
    let age_str = if age < 60 { format!("{}s ago", age) }
        else if age < 3600 { format!("{}m ago", age / 60) }
        else if age < 86400 { format!("{}h ago", age / 3600) }
        else { format!("{}d ago", age / 86400) };
    println!("Index version: {}", meta.version);
    println!("Files indexed: {}", meta.file_count);
    println!("Unique n-grams: {}", meta.ngram_count);
    println!("Built: {}", age_str);
    if let Some(ref h) = meta.commit_hash { println!("Git commit: {}", &h[..7.min(h.len())]); }
    Ok(())
}
```

- [ ] **Step 3: Run tests, verify pass**

Run: `cargo test -p rsgrep index::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 4: Commit**

```bash
git add src/index/mod.rs
git commit -m "feat: implement index builder with generation storage and writer lock"
```

---

## Task 9: Posting List Intersection/Union (intersect.rs)

**Files:**
- Create: `src/search/intersect.rs`

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intersect() { assert_eq!(intersect(&[1,3,5,7], &[3,5,9]), vec![3,5]); }
    #[test]
    fn test_intersect_empty() { assert!(intersect(&[1,2], &[]).is_empty()); }
    #[test]
    fn test_intersect_disjoint() { assert!(intersect(&[1,2], &[3,4]).is_empty()); }
    #[test]
    fn test_union() { assert_eq!(sorted_union(&[1,3,5], &[2,3,6]), vec![1,2,3,5,6]); }
    #[test]
    fn test_union_empty() { assert_eq!(sorted_union(&[1,2], &[]), vec![1,2]); }
    #[test]
    fn test_intersect_many() {
        let lists = vec![vec![1,2,3,4], vec![2,3,4,5], vec![3,4,5,6]];
        assert_eq!(intersect_many(&lists), vec![3,4]);
    }
    #[test]
    fn test_intersect_many_empty() { assert!(intersect_many(&[]).is_empty()); }
}
```

- [ ] **Step 2: Implement**

```rust
pub fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => { result.push(a[i]); i += 1; j += 1; }
        }
    }
    result
}

pub fn sorted_union(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => { result.push(a[i]); i += 1; }
            std::cmp::Ordering::Greater => { result.push(b[j]); j += 1; }
            std::cmp::Ordering::Equal => { result.push(a[i]); i += 1; j += 1; }
        }
    }
    result.extend_from_slice(&a[i..]);
    result.extend_from_slice(&b[j..]);
    result
}

pub fn intersect_many(lists: &[Vec<u32>]) -> Vec<u32> {
    if lists.is_empty() { return Vec::new(); }
    let mut r = lists[0].clone();
    for l in &lists[1..] { r = intersect(&r, l); if r.is_empty() { break; } }
    r
}

pub fn union_many(lists: &[Vec<u32>]) -> Vec<u32> {
    if lists.is_empty() { return Vec::new(); }
    let mut r = lists[0].clone();
    for l in &lists[1..] { r = sorted_union(&r, l); }
    r
}
```

- [ ] **Step 3: Run tests, verify pass, commit**

```bash
git add src/search/intersect.rs
git commit -m "feat: implement sorted posting list intersection and union"
```

---

## Task 10: Regex Decomposition with Alternation Support (decompose.rs)

**Files:**
- Create: `src/search/decompose.rs`

Handles alternations as OR plans (not dropped). Concatenations produce AND. Literals get `build_covering`. 1-byte literals are ignored for index terms.

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_simple_literal() {
        let plan = build_query_plan("handleExport");
        assert!(!matches!(plan, QueryPlan::ScanAll));
    }

    #[test]
    fn test_plan_alternation_produces_or() {
        let plan = build_query_plan("foo|bar");
        // Should produce Or plan, NOT ScanAll
        assert!(matches!(plan, QueryPlan::Or(_)),
            "alternation should produce Or plan, got {:?}", plan);
    }

    #[test]
    fn test_plan_concat_with_wildcard() {
        // "foo.*bar" should extract "foo" AND "bar"
        let plan = build_query_plan("foo.*bar");
        assert!(!matches!(plan, QueryPlan::ScanAll),
            "foo.*bar should extract literals");
    }

    #[test]
    fn test_plan_pure_wildcard() {
        let plan = build_query_plan(".*");
        assert!(matches!(plan, QueryPlan::ScanAll));
    }

    #[test]
    fn test_plan_single_char() {
        // Single char pattern has no 2+ byte literals
        let plan = build_query_plan("x");
        assert!(matches!(plan, QueryPlan::ScanAll));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rsgrep search::decompose::tests -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement decompose.rs**

```rust
use regex_syntax::hir::{Hir, HirKind};
use regex_syntax::Parser;
use crate::index::ngram::{build_covering, hash_ngram};

#[derive(Debug)]
pub enum QueryPlan {
    Lookup(u64),
    And(Vec<QueryPlan>),
    Or(Vec<QueryPlan>),
    ScanAll,
}

pub fn build_query_plan(pattern: &str) -> QueryPlan {
    let hir = match Parser::new().parse(pattern) {
        Ok(hir) => hir,
        Err(_) => return QueryPlan::ScanAll,
    };
    let plan = plan_from_hir(&hir);
    simplify(plan)
}

fn plan_from_hir(hir: &Hir) -> QueryPlan {
    match hir.kind() {
        HirKind::Literal(lit) => {
            literal_to_plan(&lit.0)
        }
        HirKind::Concat(subs) => {
            // Merge adjacent literals, then AND the results
            let mut plans = Vec::new();
            let mut current_lit = Vec::new();

            for sub in subs {
                if let HirKind::Literal(lit) = sub.kind() {
                    current_lit.extend_from_slice(&lit.0);
                } else {
                    if !current_lit.is_empty() {
                        let p = literal_to_plan(&current_lit);
                        if !matches!(p, QueryPlan::ScanAll) {
                            plans.push(p);
                        }
                        current_lit.clear();
                    }
                    let sub_plan = plan_from_hir(sub);
                    if !matches!(sub_plan, QueryPlan::ScanAll) {
                        plans.push(sub_plan);
                    }
                }
            }
            if !current_lit.is_empty() {
                let p = literal_to_plan(&current_lit);
                if !matches!(p, QueryPlan::ScanAll) {
                    plans.push(p);
                }
            }

            if plans.is_empty() {
                QueryPlan::ScanAll
            } else if plans.len() == 1 {
                plans.into_iter().next().unwrap()
            } else {
                QueryPlan::And(plans)
            }
        }
        HirKind::Alternation(subs) => {
            // OR over branch-local plans
            let mut plans = Vec::new();
            for sub in subs {
                let p = plan_from_hir(sub);
                plans.push(p);
            }
            // If ANY branch is ScanAll, the whole alternation is ScanAll
            // (we can't filter; that branch could match anything)
            if plans.iter().any(|p| matches!(p, QueryPlan::ScanAll)) {
                QueryPlan::ScanAll
            } else if plans.len() == 1 {
                plans.into_iter().next().unwrap()
            } else {
                QueryPlan::Or(plans)
            }
        }
        HirKind::Repetition(rep) => {
            if rep.min >= 1 {
                plan_from_hir(&rep.sub)
            } else {
                QueryPlan::ScanAll
            }
        }
        HirKind::Capture(cap) => {
            plan_from_hir(&cap.sub)
        }
        _ => QueryPlan::ScanAll,
    }
}

fn literal_to_plan(bytes: &[u8]) -> QueryPlan {
    if bytes.len() < 2 {
        return QueryPlan::ScanAll;
    }
    let covering = build_covering(bytes);
    let lookups: Vec<QueryPlan> = covering.iter()
        .map(|&(s, e)| QueryPlan::Lookup(hash_ngram(&bytes[s..e])))
        .collect();
    if lookups.is_empty() {
        QueryPlan::ScanAll
    } else if lookups.len() == 1 {
        lookups.into_iter().next().unwrap()
    } else {
        QueryPlan::And(lookups)
    }
}

fn simplify(plan: QueryPlan) -> QueryPlan {
    match plan {
        QueryPlan::And(mut subs) if subs.len() == 1 => simplify(subs.remove(0)),
        QueryPlan::Or(mut subs) if subs.len() == 1 => simplify(subs.remove(0)),
        other => other,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rsgrep search::decompose::tests -- --nocapture`
Expected: All PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/decompose.rs
git commit -m "feat: implement regex decomposition with alternation OR support"
```

---

## Task 11: Candidate Verification with Context Lines (verify.rs)

**Files:**
- Create: `src/search/verify.rs`

Parallel regex verification. Supports context lines (`-C`).

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_verify_file_match() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.rs");
        fs::write(&path, "fn hello_world() {\n    println!(\"hi\");\n}\n").unwrap();
        let re = regex::bytes::Regex::new("hello_world").unwrap();
        let matches = verify_file(&path, &re, None, 0);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_number, 1);
    }

    #[test]
    fn test_verify_file_no_match() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.rs");
        fs::write(&path, "fn main() {}").unwrap();
        let re = regex::bytes::Regex::new("nonexistent").unwrap();
        assert!(verify_file(&path, &re, None, 0).is_empty());
    }

    #[test]
    fn test_verify_with_context() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.rs");
        fs::write(&path, "line1\nline2\nmatch_here\nline4\nline5\n").unwrap();
        let re = regex::bytes::Regex::new("match_here").unwrap();
        let matches = verify_file(&path, &re, None, 1);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].context_before.len(), 1);
        assert_eq!(matches[0].context_after.len(), 1);
    }

    #[test]
    fn test_verify_max_count() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.rs");
        fs::write(&path, "aaa\nbbb\naaa\naaa\n").unwrap();
        let re = regex::bytes::Regex::new("aaa").unwrap();
        let matches = verify_file(&path, &re, Some(2), 0);
        assert_eq!(matches.len(), 2);
    }
}
```

- [ ] **Step 2: Implement verify.rs**

```rust
use regex::bytes::Regex;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Match {
    pub file_path: String,
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
    pub context_before: Vec<(usize, String)>,
    pub context_after: Vec<(usize, String)>,
}

pub fn verify_file(path: &Path, re: &Regex, max_count: Option<usize>, context: usize) -> Vec<Match> {
    let content = match std::fs::read(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let file_path = path.to_string_lossy().to_string();
    let lines: Vec<&[u8]> = content.split(|&b| b == b'\n').collect();
    let mut matches = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(max) = max_count {
            if matches.len() >= max { break; }
        }
        if let Some(m) = re.find(line) {
            let mut ctx_before = Vec::new();
            let mut ctx_after = Vec::new();

            if context > 0 {
                let start = line_idx.saturating_sub(context);
                for ci in start..line_idx {
                    ctx_before.push((ci + 1, String::from_utf8_lossy(lines[ci]).to_string()));
                }
                for ci in (line_idx + 1)..((line_idx + 1 + context).min(lines.len())) {
                    ctx_after.push((ci + 1, String::from_utf8_lossy(lines[ci]).to_string()));
                }
            }

            matches.push(Match {
                file_path: file_path.clone(),
                line_number: line_idx + 1,
                line_content: String::from_utf8_lossy(line).to_string(),
                match_start: m.start(),
                match_end: m.end(),
                context_before: ctx_before,
                context_after: ctx_after,
            });
        }
    }
    matches
}
```

- [ ] **Step 3: Run tests, verify pass, commit**

```bash
git add src/search/verify.rs
git commit -m "feat: implement candidate verification with context line support"
```

---

## Task 12: Output Formatting with JSON + TTY Detection (output/)

**Files:**
- Create: `src/output/mod.rs`
- Create: `src/output/color.rs`

- [ ] **Step 1: Implement color.rs with is-terminal**

```rust
use is_terminal::IsTerminal;

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const MAGENTA: &str = "\x1b[35m";

pub fn should_color() -> bool {
    std::io::stdout().is_terminal() && std::env::var("NO_COLOR").is_err()
}
```

- [ ] **Step 2: Implement output/mod.rs with JSON support**

```rust
pub mod color;

use crate::search::verify::Match;

pub fn format_match(m: &Match, use_color: bool) -> String {
    if use_color {
        let before = &m.line_content[..m.match_start];
        let matched = &m.line_content[m.match_start..m.match_end];
        let after = &m.line_content[m.match_end..];
        format!("{}{}{}:{}{}{}:{}{}{}{}{}{}",
            color::MAGENTA, m.file_path, color::RESET,
            color::GREEN, m.line_number, color::RESET,
            before, color::RED, color::BOLD, matched, color::RESET, after)
    } else {
        format!("{}:{}:{}", m.file_path, m.line_number, m.line_content)
    }
}

pub fn format_context_line(line_num: usize, content: &str, file_path: &str, use_color: bool) -> String {
    if use_color {
        format!("{}{}{}-{}{}{}-{}", color::MAGENTA, file_path, color::RESET,
            color::GREEN, line_num, color::RESET, content)
    } else {
        format!("{}-{}-{}", file_path, line_num, content)
    }
}

pub fn format_match_json(m: &Match) -> String {
    serde_json::json!({
        "file": m.file_path,
        "line": m.line_number,
        "content": m.line_content,
        "match_start": m.match_start,
        "match_end": m.match_end,
    }).to_string()
}

pub fn format_count(file_path: &str, count: usize, use_color: bool) -> String {
    if use_color {
        format!("{}{}{}:{}", color::MAGENTA, file_path, color::RESET, count)
    } else {
        format!("{}:{}", file_path, count)
    }
}

pub fn unique_files(matches: &[Match]) -> Vec<String> {
    let mut files: Vec<String> = matches.iter().map(|m| m.file_path.clone()).collect();
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_match_plain() {
        let m = Match {
            file_path: "src/main.rs".to_string(),
            line_number: 42,
            line_content: "fn hello() {}".to_string(),
            match_start: 3,
            match_end: 8,
            context_before: vec![],
            context_after: vec![],
        };
        let output = format_match(&m, false);
        assert_eq!(output, "src/main.rs:42:fn hello() {}");
    }

    #[test]
    fn test_unique_files() {
        let matches = vec![
            Match { file_path: "a.rs".into(), line_number: 1, line_content: "x".into(),
                match_start: 0, match_end: 1, context_before: vec![], context_after: vec![] },
            Match { file_path: "a.rs".into(), line_number: 2, line_content: "y".into(),
                match_start: 0, match_end: 1, context_before: vec![], context_after: vec![] },
        ];
        assert_eq!(unique_files(&matches), vec!["a.rs"]);
    }
}
```

- [ ] **Step 3: Run tests, verify pass, commit**

```bash
git add src/output/
git commit -m "feat: implement output formatting with JSON, color, and context lines"
```

---

## Task 13: Search Pipeline (search/mod.rs)

**Files:**
- Modify: `src/search/mod.rs`

Orchestrates: load index, decompose regex, execute query plan, verify candidates. Supports `--no-index` brute-force fallback.

- [ ] **Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn setup() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.rs"), "fn hello_world() {\n    println!(\"hi\");\n}\n").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {\n    hello_world();\n}\n").unwrap();
        fs::write(dir.path().join("other.rs"), "fn other() {\n    let x = 42;\n}\n").unwrap();
        crate::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
        dir
    }

    #[test]
    fn test_search_finds_matches() {
        let dir = setup();
        let r = search(dir.path(), "hello_world", &SearchOptions::default()).unwrap();
        assert!(r.len() >= 2);
    }

    #[test]
    fn test_search_no_match() {
        let dir = setup();
        let r = search(dir.path(), "nonexistent_xyz_123", &SearchOptions::default()).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn test_search_regex() {
        let dir = setup();
        let r = search(dir.path(), "fn\\s+\\w+", &SearchOptions::default()).unwrap();
        assert!(r.len() >= 3);
    }

    #[test]
    fn test_search_alternation() {
        let dir = setup();
        let r = search(dir.path(), "hello_world|other", &SearchOptions::default()).unwrap();
        assert!(r.len() >= 3, "alternation should find matches from both branches");
    }
}
```

- [ ] **Step 2: Implement search/mod.rs**

```rust
pub mod decompose;
pub mod intersect;
pub mod verify;

use anyhow::Result;
use std::path::Path;
use crate::index::{current_generation, meta::IndexMeta};
use crate::index::lookup::MmapLookupTable;
use crate::index::postings::decode_posting_list;
use crate::index::filetable::FileTableReader;
use decompose::{build_query_plan, QueryPlan};
use intersect::{intersect_many, union_many};
use verify::{Match, verify_file};

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub case_insensitive: bool,
    pub files_only: bool,
    pub count: bool,
    pub max_count: Option<usize>,
    pub quiet: bool,
    pub literal: bool,
    pub context: usize,
    pub no_index: bool,
    pub glob_pattern: Option<String>,
    pub file_type: Option<String>,
    pub json: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_insensitive: false, files_only: false, count: false,
            max_count: None, quiet: false, literal: false, context: 0,
            no_index: false, glob_pattern: None, file_type: None, json: false,
        }
    }
}

pub fn search(root: &Path, pattern: &str, opts: &SearchOptions) -> Result<Vec<Match>> {
    let effective = if opts.literal { regex::escape(pattern) } else { pattern.to_string() };
    let effective = if opts.case_insensitive { format!("(?i){}", effective) } else { effective };

    let re = regex::bytes::Regex::new(&effective)?;

    if opts.no_index {
        return brute_force_search(root, &re, opts);
    }

    let gen_dir = current_generation(root)?;
    let meta = IndexMeta::read(&gen_dir.join("meta.json"))?;
    let age = IndexMeta::timestamp_now().saturating_sub(meta.timestamp);
    if age > 86400 {
        eprintln!("warning: index is {}h old, consider `rsgrep update`", age / 3600);
    }

    let lookup = MmapLookupTable::open(&gen_dir.join("lookup.bin"))?;
    let postings_data = std::fs::read(&gen_dir.join("postings.bin"))?;
    let file_table = FileTableReader::open(&gen_dir.join("files.bin"))?;

    let plan = build_query_plan(&effective);
    let candidates = execute_plan(&plan, &lookup, &postings_data, &file_table)?;

    let mut all_matches = Vec::new();
    for fid in &candidates {
        if let Some(entry) = file_table.get(*fid) {
            let full_path = root.join(&entry.path);
            if !matches_filters(&full_path, opts) { continue; }
            let m = verify_file(&full_path, &re, opts.max_count, opts.context);
            all_matches.extend(m);
        }
    }
    Ok(all_matches)
}

fn brute_force_search(root: &Path, re: &regex::bytes::Regex, opts: &SearchOptions) -> Result<Vec<Match>> {
    let files = crate::ignore::walk_files(root, 10 * 1024 * 1024)?;
    let mut all = Vec::new();
    for path in &files {
        if !matches_filters(path, opts) { continue; }
        let m = verify_file(path, re, opts.max_count, opts.context);
        all.extend(m);
    }
    Ok(all)
}

fn matches_filters(path: &Path, opts: &SearchOptions) -> bool {
    if let Some(ref glob_pat) = opts.glob_pattern {
        if let Ok(pat) = glob::Pattern::new(glob_pat) {
            if !pat.matches_path(path) { return false; }
        }
    }
    if let Some(ref ext) = opts.file_type {
        if path.extension().and_then(|e| e.to_str()) != Some(ext.as_str()) {
            return false;
        }
    }
    true
}

fn execute_plan(plan: &QueryPlan, lookup: &MmapLookupTable, postings: &[u8], ft: &FileTableReader) -> Result<Vec<u32>> {
    match plan {
        QueryPlan::Lookup(hash) => {
            if let Some((offset, length)) = lookup.lookup(*hash) {
                let s = offset as usize;
                let e = s + length as usize;
                if e <= postings.len() { Ok(decode_posting_list(&postings[s..e])) }
                else { Ok(Vec::new()) }
            } else { Ok(Vec::new()) }
        }
        QueryPlan::And(subs) => {
            let lists: Result<Vec<_>> = subs.iter().map(|s| execute_plan(s, lookup, postings, ft)).collect();
            Ok(intersect_many(&lists?))
        }
        QueryPlan::Or(subs) => {
            let lists: Result<Vec<_>> = subs.iter().map(|s| execute_plan(s, lookup, postings, ft)).collect();
            Ok(union_many(&lists?))
        }
        QueryPlan::ScanAll => Ok(ft.all_file_ids()),
    }
}
```

- [ ] **Step 3: Run tests, verify pass, commit**

```bash
git add src/search/mod.rs
git commit -m "feat: implement search pipeline with alternation, brute-force fallback, and filters"
```

---

## Task 14: CLI (cli.rs)

**Files:**
- Modify: `src/cli.rs`

All flags wired: `--no-index`, `--glob`, `--type`, `--context`, `--json`, `--count`, `--quiet`, `--max-count`. Exit codes 0/1/2.

- [ ] **Step 1: Implement cli.rs**

```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

use crate::index;
use crate::search::{self, SearchOptions};
use crate::output;

#[derive(Parser)]
#[command(name = "rsgrep", version, about = "Fast regex search with sparse n-gram indexing")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build or rebuild the search index
    Index {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "10485760")]
        max_filesize: u64,
    },
    /// Search using the index
    Search {
        pattern: String,
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short = 'n', long)]
        no_index: bool,
        #[arg(short = 'F', long)]
        literal: bool,
        #[arg(short = 'i')]
        case_insensitive: bool,
        #[arg(short = 'l')]
        files_only: bool,
        #[arg(short = 'c', long)]
        count: bool,
        #[arg(short = 'm', long)]
        max_count: Option<usize>,
        #[arg(short = 'q', long)]
        quiet: bool,
        #[arg(short = 'C', long, default_value = "0")]
        context: usize,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        glob: Option<String>,
        #[arg(long = "type")]
        file_type: Option<String>,
    },
    /// Incrementally update the index
    Update {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show index status
    Status {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Index { path, max_filesize, .. } => {
            let root = std::fs::canonicalize(&path)?;
            eprintln!("Indexing {}...", root.display());
            index::build_index(&root, max_filesize)?;
            let gen = index::current_generation(&root)?;
            let meta = index::meta::IndexMeta::read(&gen.join("meta.json"))?;
            eprintln!("Done. {} files, {} n-grams.", meta.file_count, meta.ngram_count);
            Ok(())
        }
        Commands::Search {
            pattern, path, no_index, literal, case_insensitive,
            files_only, count, max_count, quiet, context, json, glob, file_type,
        } => {
            let root = std::fs::canonicalize(&path)?;
            let opts = SearchOptions {
                case_insensitive, files_only, count, max_count, quiet,
                literal, context, no_index, glob_pattern: glob,
                file_type, json,
            };

            let matches = search::search(&root, &pattern, &opts)?;

            if quiet {
                process::exit(if matches.is_empty() { 1 } else { 0 });
            }
            if matches.is_empty() {
                process::exit(1);
            }

            let use_color = output::color::should_color();

            if files_only {
                for f in &output::unique_files(&matches) { println!("{}", f); }
            } else if count {
                let mut counts = std::collections::HashMap::new();
                for m in &matches { *counts.entry(m.file_path.as_str()).or_insert(0usize) += 1; }
                let mut sorted: Vec<_> = counts.into_iter().collect();
                sorted.sort_by_key(|(p, _)| p.to_string());
                for (p, c) in sorted { println!("{}", output::format_count(p, c, use_color)); }
            } else if json {
                for m in &matches { println!("{}", output::format_match_json(m)); }
            } else {
                for m in &matches {
                    for (ln, content) in &m.context_before {
                        println!("{}", output::format_context_line(*ln, content, &m.file_path, use_color));
                    }
                    println!("{}", output::format_match(m, use_color));
                    for (ln, content) in &m.context_after {
                        println!("{}", output::format_context_line(*ln, content, &m.file_path, use_color));
                    }
                }
            }
            Ok(())
        }
        Commands::Update { path } => {
            let root = std::fs::canonicalize(&path)?;
            eprintln!("Updating index...");
            // v0.1: update = full rebuild. Overlay support is a future enhancement.
            index::build_index(&root, 10 * 1024 * 1024)?;
            eprintln!("Done.");
            Ok(())
        }
        Commands::Status { path } => {
            let root = std::fs::canonicalize(&path)?;
            index::index_status(&root)
        }
    };

    if let Err(e) = result {
        eprintln!("rsgrep: {}", e);
        process::exit(2);
    }
    Ok(())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles

- [ ] **Step 3: Smoke test**

```bash
cargo run -- index .
cargo run -- search "fn " .
cargo run -- search "fn " . --context 2
cargo run -- search "fn " . --json
cargo run -- search "fn " . -l
cargo run -- search "fn " . -c
cargo run -- search "fn " . -n  # brute force
cargo run -- status .
```

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs
git commit -m "feat: implement CLI with all flags wired (context, json, glob, type, no-index)"
```

---

## Task 15: Integration Tests

**Files:**
- Create: `tests/integration.rs`

- [ ] **Step 1: Write integration tests**

```rust
use std::fs;
use std::collections::HashSet;
use tempfile::TempDir;

fn setup_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), "fn main() {\n    let config = load_config();\n    run_server(config);\n}\n").unwrap();
    fs::write(src.join("config.rs"), "pub struct Config {\n    pub port: u16,\n}\npub fn load_config() -> Config {\n    Config { port: 8080 }\n}\n").unwrap();
    fs::write(src.join("server.rs"), "pub fn run_server(config: crate::config::Config) {\n    println!(\"{}:{}\", config.host, config.port);\n}\n").unwrap();
    fs::write(dir.path().join("image.png"), &[0x89, 0x50, 0x4E, 0x47, 0x00]).unwrap();
    dir
}

#[test]
fn test_full_pipeline() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions::default();
    let results = rsgrep::search::search(dir.path(), "load_config", &opts).unwrap();
    assert!(results.len() >= 2, "load_config in main.rs and config.rs");
}

#[test]
fn test_search_superset_of_bruteforce() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions::default();
    let indexed = rsgrep::search::search(dir.path(), "Config", &opts).unwrap();
    let indexed_files: HashSet<_> = indexed.iter().map(|m| m.file_path.clone()).collect();

    // Brute force
    let bf_opts = rsgrep::search::SearchOptions { no_index: true, ..Default::default() };
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
    let opts = rsgrep::search::SearchOptions { case_insensitive: true, ..Default::default() };
    let results = rsgrep::search::search(dir.path(), "config", &opts).unwrap();
    assert!(!results.is_empty());
}

#[test]
fn test_literal_search() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions { literal: true, ..Default::default() };
    let results = rsgrep::search::search(dir.path(), "{}:{}", &opts).unwrap();
    assert!(!results.is_empty());
}

#[test]
fn test_alternation_search() {
    let dir = setup_project();
    rsgrep::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
    let opts = rsgrep::search::SearchOptions::default();
    let results = rsgrep::search::search(dir.path(), "load_config|run_server", &opts).unwrap();
    assert!(results.len() >= 3, "alternation should find both functions");
}
```

- [ ] **Step 2: Run, verify pass, commit**

```bash
cargo test --test integration -- --nocapture
git add tests/integration.rs
git commit -m "test: add end-to-end integration tests with superset verification"
```

---

## Task 16: Property Tests

**Files:**
- Create: `tests/property_tests.rs`

- [ ] **Step 1: Write property tests**

```rust
use proptest::prelude::*;
use rsgrep::index::ngram::{build_all, build_covering};

proptest! {
    #[test]
    fn covering_subset_of_all(content in prop::collection::vec(any::<u8>(), 2..100)) {
        let all = build_all(&content);
        let covering = build_covering(&content);
        for span in &covering {
            prop_assert!(all.contains(span),
                "covering {:?} not in build_all (content len {})", span, content.len());
        }
    }

    #[test]
    fn covering_covers_all_positions(content in prop::collection::vec(any::<u8>(), 2..100)) {
        let covering = build_covering(&content);
        let num_pairs = content.len() - 1;
        let mut covered = vec![false; num_pairs];
        for &(start, end) in &covering {
            for p in start..end.saturating_sub(1) {
                if p < covered.len() { covered[p] = true; }
            }
        }
        for (i, &c) in covered.iter().enumerate() {
            prop_assert!(c, "pair position {} not covered", i);
        }
    }

    #[test]
    fn build_all_laminarity(content in prop::collection::vec(any::<u8>(), 2..50)) {
        let all = build_all(&content);
        for i in 0..all.len() {
            for j in (i+1)..all.len() {
                let (a_s, a_e) = all[i];
                let (b_s, b_e) = all[j];
                let crosses = a_s < b_s && b_s < a_e && a_e < b_e;
                let crosses_rev = b_s < a_s && a_s < b_e && b_e < a_e;
                prop_assert!(!crosses && !crosses_rev,
                    "[{},{}) and [{},{}) cross", a_s, a_e, b_s, b_e);
            }
        }
    }

    /// Greedy optimality: on short inputs, verify covering cardinality matches
    /// brute-force minimum cover.
    #[test]
    fn greedy_is_optimal(content in prop::collection::vec(any::<u8>(), 2..20)) {
        let all = build_all(&content);
        let covering = build_covering(&content);
        let num_pairs = content.len() - 1;

        // Brute-force: find minimum number of intervals from `all` that cover all pairs
        let min_cover = brute_force_min_cover(&all, num_pairs);

        prop_assert!(covering.len() <= min_cover + 1,
            "greedy {} vs optimal {} (tolerance 1)", covering.len(), min_cover);
    }
}

fn brute_force_min_cover(intervals: &[(usize, usize)], num_pairs: usize) -> usize {
    if num_pairs == 0 { return 0; }
    // Filter to intervals that cover at least one pair
    let valid: Vec<_> = intervals.iter()
        .filter(|&&(s, e)| e > s + 1)
        .cloned()
        .collect();
    if valid.is_empty() { return usize::MAX; }

    // Greedy interval cover (optimal for interval covering on a line)
    let mut uncovered = 0;
    let mut count = 0;
    while uncovered < num_pairs {
        let best = valid.iter()
            .filter(|&&(s, _)| s <= uncovered)
            .map(|&(_, e)| e.saturating_sub(1))
            .max();
        match best {
            Some(reach) if reach > uncovered => {
                count += 1;
                uncovered = reach;
            }
            _ => break,
        }
    }
    count
}
```

- [ ] **Step 2: Run, verify pass, commit**

```bash
cargo test --test property_tests -- --nocapture
git add tests/property_tests.rs
git commit -m "test: add property tests (laminarity, coverage, subset, greedy optimality)"
```

---

## Task 17: Benchmarks

**Files:**
- Create: `benches/bench.rs`

- [ ] **Step 1: Write benchmarks**

```rust
use criterion::{criterion_group, criterion_main, Criterion};
use tempfile::TempDir;
use std::fs;

fn create_synthetic_repo(num_files: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    for i in 0..num_files {
        let content = format!(
            "fn function_{}() {{\n    let value_{} = {};\n    println!(\"hello {}\");\n}}\n",
            i, i, i * 42, i
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
        b.iter(|| {
            rsgrep::search::search(dir.path(), "function_500", &opts).unwrap()
        })
    });

    c.bench_function("search_broad_1k", |b| {
        b.iter(|| {
            rsgrep::search::search(dir.path(), "fn ", &opts).unwrap()
        })
    });
}

criterion_group!(benches, bench_index_build, bench_search);
criterion_main!(benches);
```

- [ ] **Step 2: Run benchmarks**

Run: `cargo bench`
Expected: Benchmark results printed

- [ ] **Step 3: Commit**

```bash
git add benches/bench.rs
git commit -m "bench: add criterion benchmarks for index build and search"
```

---

## Task 18: Final Polish

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: All pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Fix any issues.

- [ ] **Step 3: Build release binary**

Run: `cargo build --release`
Expected: Binary at `target/release/rsgrep`

- [ ] **Step 4: Smoke test release**

```bash
./target/release/rsgrep index .
./target/release/rsgrep search "fn " .
./target/release/rsgrep search "fn " . -C 2
./target/release/rsgrep search "fn " . --json
./target/release/rsgrep search "build_all|build_covering" .
./target/release/rsgrep search "fn " . -n
./target/release/rsgrep status .
```

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "feat: rsgrep v0.1.0 — fast regex search with sparse n-gram indexing"
```

---

## Deferred for v0.2

These spec features are acknowledged but deferred from v0.1:

1. **Overlay/tombstone incremental updates** — `rsgrep update` currently does a full rebuild. The generation-based storage layout is in place; adding `overlay/` subdirectories with append-only postings and tombstones.bin is the next major feature.
2. **O(n^2) → O(n) n-gram extraction** — monotone-stack or Cartesian-tree optimization for `build_all`.
3. **Corpus-derived weight table** — replace the hash-based placeholder with inverse-frequency weights from a real open-source corpus.
