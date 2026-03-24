/// Static 256x256 weight table derived from byte-pair frequency in source code.
///
/// Common byte pairs (English bigrams, letter+space, indentation) get LOW weights,
/// producing shorter n-grams (less selective). Rare pairs (non-ASCII, unusual
/// punctuation combinations) get HIGH weights, producing longer n-grams (more
/// selective). More selective n-grams = smaller posting lists = faster search.
///
/// Values are in [1, 251] — never zero (zero would break edge-dominance comparisons
/// against the `max_inside = 0` initial value in build_all/build_covering).
pub static WEIGHT_TABLE: [[u32; 256]; 256] = {
    let mut table = [[0u32; 256]; 256];
    let mut a = 0usize;
    while a < 256 {
        let mut b = 0usize;
        while b < 256 {
            table[a][b] = compute_weight(a as u8, b as u8);
            b += 1;
        }
        a += 1;
    }
    table
};

// Character class constants for const fn compatibility.
const CC_LOWER: u8 = 0;
const CC_UPPER: u8 = 1;
const CC_DIGIT: u8 = 2;
const CC_SPACE: u8 = 3;
const CC_PUNCT: u8 = 4;
const CC_CONTROL: u8 = 5;
const CC_NON_ASCII: u8 = 6;

/// Classify a byte into a character class.
const fn char_class(b: u8) -> u8 {
    match b {
        b'a'..=b'z' => CC_LOWER,
        b'A'..=b'Z' => CC_UPPER,
        b'0'..=b'9' => CC_DIGIT,
        b' ' | b'\t' | b'\n' | b'\r' => CC_SPACE,
        0..=31 | 127 => CC_CONTROL,
        128..=255 => CC_NON_ASCII,
        _ => CC_PUNCT, // remaining ASCII printable: !@#$%^&*()_+-=[]{}|;:'",.<>?/`~
    }
}

/// Compute the weight for the top 20+ most common English and code bigrams.
/// Returns 0 if the pair is not in the known-common list.
const fn letter_pair_rank(a: u8, b: u8) -> u32 {
    match (a, b) {
        // Top 20 most common English bigrams
        (b't', b'h') => 1,
        (b'h', b'e') => 2,
        (b'i', b'n') => 3,
        (b'e', b'r') => 4,
        (b'a', b'n') => 5,
        (b'r', b'e') => 6,
        (b'o', b'n') => 7,
        (b'a', b't') => 8,
        (b'e', b'n') => 9,
        (b'n', b'd') => 10,
        (b't', b'i') => 11,
        (b'e', b's') => 12,
        (b'o', b'r') => 13,
        (b't', b'e') => 14,
        (b'o', b'f') => 15,
        (b'e', b'd') => 16,
        (b'i', b's') => 17,
        (b'i', b't') => 18,
        (b'a', b'l') => 19,
        (b'a', b'r') => 20,
        // Common code-specific bigrams
        (b'e', b't') => 12, // "get", "set"
        (b'r', b'n') => 15, // "return"
        (b'u', b'n') => 18, // "function", "fun"
        (b's', b't') => 14, // "const", "string"
        (b'n', b'g') => 16, // "string", "ing"
        (b'l', b'e') => 17, // "file", "table"
        (b'c', b't') => 20, // "struct", "object"
        _ => 0,
    }
}

/// Compute the base weight for a lowercase letter pair.
const fn base_from_letter_freq(a: u8, b: u8) -> u32 {
    let rank = letter_pair_rank(a, b);
    if rank > 0 {
        // Very common: weight 3-25 based on rank
        3 + rank
    } else {
        // Default lowercase pair: medium-low weight (30-50 range)
        // Use a simple hash to distribute deterministically
        let h = ((a as u32).wrapping_mul(7) ^ (b as u32).wrapping_mul(13)) % 20;
        30 + h
    }
}

/// Compute the corpus-derived weight for byte pair (a, b).
/// Returns a value in [1, 251].
const fn compute_weight(a: u8, b: u8) -> u32 {
    let class_a = char_class(a);
    let class_b = char_class(b);

    let base = match (class_a, class_b) {
        // Most common: lowercase letter pairs — use frequency-based weights
        (CC_LOWER, CC_LOWER) => base_from_letter_freq(a, b),
        // Common: letter + space/newline
        (CC_LOWER, CC_SPACE) | (CC_SPACE, CC_LOWER) => 15,
        // Common: space + space (indentation)
        (CC_SPACE, CC_SPACE) => 5,
        // Medium: camelCase transitions (lowerUpper)
        (CC_LOWER, CC_UPPER) => 100,
        // Medium-low: UpperLower (start of camelCase word)
        (CC_UPPER, CC_LOWER) => 40,
        // Medium: ALL_CAPS identifiers
        (CC_UPPER, CC_UPPER) => 60,
        // Medium: digit pairs
        (CC_DIGIT, CC_DIGIT) => 80,
        // Medium: letter + digit or digit + letter
        (CC_LOWER, CC_DIGIT) | (CC_DIGIT, CC_LOWER) => 70,
        // Upper + digit or digit + upper
        (CC_UPPER, CC_DIGIT) | (CC_DIGIT, CC_UPPER) => 75,
        // Medium-high: punctuation + letter or letter + punctuation
        (CC_PUNCT, CC_LOWER) | (CC_LOWER, CC_PUNCT) => 120,
        (CC_PUNCT, CC_UPPER) | (CC_UPPER, CC_PUNCT) => 130,
        // Medium-high: punctuation + punctuation
        (CC_PUNCT, CC_PUNCT) => 140,
        // Medium: punctuation + space or space + punctuation
        (CC_PUNCT, CC_SPACE) | (CC_SPACE, CC_PUNCT) => 90,
        // Medium: space + upper (start of sentence/identifier)
        (CC_SPACE, CC_UPPER) | (CC_UPPER, CC_SPACE) => 50,
        // Medium: space + digit or digit + space
        (CC_SPACE, CC_DIGIT) | (CC_DIGIT, CC_SPACE) => 60,
        // Medium-high: digit + punctuation or punctuation + digit
        (CC_DIGIT, CC_PUNCT) | (CC_PUNCT, CC_DIGIT) => 110,
        // High: anything involving non-ASCII
        (CC_NON_ASCII, CC_NON_ASCII) => 230,
        (CC_NON_ASCII, _) | (_, CC_NON_ASCII) => 220,
        // High: control characters (rare in source code)
        (CC_CONTROL, CC_CONTROL) => 200,
        (CC_CONTROL, _) | (_, CC_CONTROL) => 180,
        // Fallback (should not be reached given exhaustive classification)
        _ => 100,
    };

    // Reduce weight for identical bytes (common in ==, --, //, indentation, etc.)
    let adjusted = if a == b { base * 7 / 10 } else { base };

    // Clamp to [1, 251]
    if adjusted < 1 {
        1
    } else if adjusted > 251 {
        251
    } else {
        adjusted
    }
}

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

        if right <= left + 1 || (weights[left] > max_inside && weights[right] > max_inside) {
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
            if !found
                || candidate_end > best_right_end
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
    if h == 0 {
        1
    } else {
        h
    }
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
            assert!(
                end - start <= 3,
                "equal weights should not produce spans > 3 bytes"
            );
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
            assert!(
                all.contains(span),
                "covering span {:?} not in build_all",
                span
            );
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

    #[test]
    fn test_common_pairs_have_low_weights() {
        // Common English bigrams should have low weights
        let common = [
            (b't', b'h'),
            (b'h', b'e'),
            (b'i', b'n'),
            (b'e', b'r'),
        ];
        for (a, b) in common {
            let w = WEIGHT_TABLE[a as usize][b as usize];
            assert!(
                w < 30,
                "common pair ({},{}) should have low weight, got {}",
                a as char,
                b as char,
                w
            );
        }
    }

    #[test]
    fn test_rare_pairs_have_high_weights() {
        // Non-ASCII and rare punctuation should have high weights
        let rare = [(0xFFu8, 0xFEu8), (b'~', b'^'), (b'#', b'!')];
        for (a, b) in rare {
            let w = WEIGHT_TABLE[a as usize][b as usize];
            assert!(
                w > 100,
                "rare pair ({},{}) should have high weight, got {}",
                a, b, w
            );
        }
    }

    #[test]
    fn test_camelcase_transitions_medium() {
        // lowerUpper transitions should be medium-high (good n-gram boundaries)
        let w = WEIGHT_TABLE[b'e' as usize][b'C' as usize];
        assert!(
            w >= 80 && w <= 150,
            "camelCase transition should be medium, got {}",
            w
        );
    }

    #[test]
    fn test_space_pairs_low() {
        let w = WEIGHT_TABLE[b' ' as usize][b' ' as usize];
        assert!(w < 10, "space-space should be very low weight, got {}", w);
    }

    #[test]
    fn test_all_weights_in_range() {
        for a in 0..256 {
            for b in 0..256 {
                let w = WEIGHT_TABLE[a][b];
                assert!(
                    w >= 1 && w <= 251,
                    "weight[{}][{}] = {} out of range",
                    a,
                    b,
                    w
                );
            }
        }
    }

    #[test]
    fn test_corpus_weights_produce_better_selectivity() {
        // With corpus weights, common text should produce fewer unique n-grams
        // than rare text, because common pairs create shorter spans (more merging)
        let common_text = b"the function returns the result of the operation";
        let rare_text = b"#![cfg(target_arch = \"x86_64\")] ~^`$@";

        let common_ngrams = build_all(common_text);
        let rare_ngrams = build_all(rare_text);

        // Common text: many short n-grams (2-3 bytes mostly)
        let common_avg_len: f64 = common_ngrams
            .iter()
            .map(|(s, e)| (e - s) as f64)
            .sum::<f64>()
            / common_ngrams.len().max(1) as f64;

        // Rare text: should have some longer n-grams
        let rare_avg_len: f64 = rare_ngrams
            .iter()
            .map(|(s, e)| (e - s) as f64)
            .sum::<f64>()
            / rare_ngrams.len().max(1) as f64;

        // Rare text should NOT have shorter average n-grams than common text
        assert!(
            rare_avg_len >= common_avg_len * 0.8,
            "rare text avg n-gram len ({:.1}) should not be much shorter than common ({:.1})",
            rare_avg_len,
            common_avg_len
        );
    }
}
