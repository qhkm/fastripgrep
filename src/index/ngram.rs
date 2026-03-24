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
        let limit = weights.len().min(left + MAX_NGRAM_LEN - 1);

        for right in left..limit {
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

/// Maximum n-gram length in bytes to consider. Limits O(n) scan in widest_from
/// to a constant, making build_covering O(n) overall.
const MAX_NGRAM_LEN: usize = 64;

/// Find the widest sparse n-gram starting at `left` in pair-space.
/// Returns the largest valid right endpoint. Capped at MAX_NGRAM_LEN bytes.
fn widest_from(weights: &[u32], left: usize) -> usize {
    let mut max_inside: u32 = 0;
    let mut best = left;
    let limit = weights.len().min(left + MAX_NGRAM_LEN - 1);

    for right in left..limit {
        if right >= left + 2 {
            max_inside = max_inside.max(weights[right - 1]);
        }

        if right <= left + 1
            || (weights[left] > max_inside && weights[right] > max_inside)
        {
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

/// Hash an n-gram byte slice to a u64 using xxh3 (stable across Rust versions).
/// Never returns 0 (reserved for empty lookup table slots).
pub fn hash_ngram(bytes: &[u8]) -> u64 {
    let h = xxhash_rust::xxh3::xxh3_64(bytes);
    if h == 0 { 1 } else { h }
}

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
}
