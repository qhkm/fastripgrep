use proptest::prelude::*;
use proptest::test_runner::Config as ProptestConfig;
use fastripgrep::index::ngram::{build_all, build_covering};

proptest! {
    #![proptest_config(ProptestConfig {
        failure_persistence: None,
        .. ProptestConfig::default()
    })]

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

    /// Laminarity: no two sparse n-grams from build_all cross in pair-space
    /// (strictly interior crossing -- shared edge pairs are allowed).
    ///
    /// Byte span (s, e) corresponds to pair-space interval [s, e-2].
    /// Two pair-space intervals [a_s, a_e] and [b_s, b_e] strictly cross iff:
    ///   a_s < b_s AND b_s < a_e AND a_e < b_e (all strict <)
    ///
    /// The strict < at the middle position means spans that merely touch
    /// (share exactly one edge pair) are NOT considered crossing.
    /// This invariant holds because if b_s is strictly interior to A and
    /// a_e is strictly interior to B, the edge-dominance conditions
    /// produce a contradiction.
    #[test]
    fn build_all_laminarity(content in prop::collection::vec(any::<u8>(), 2..50)) {
        let all = build_all(&content);
        for i in 0..all.len() {
            for j in (i+1)..all.len() {
                let (a_s, a_e) = all[i];
                let (b_s, b_e) = all[j];
                // Convert byte-space to pair-space
                if a_e < 2 || b_e < 2 { continue; }
                let a_pe = a_e - 2; // rightmost pair position
                let b_pe = b_e - 2;

                // Strict crossing in pair-space: all strict <
                let crosses = a_s < b_s && b_s < a_pe && a_pe < b_pe;
                let crosses_rev = b_s < a_s && a_s < b_pe && b_pe < a_pe;
                prop_assert!(!crosses && !crosses_rev,
                    "byte [{},{}) pair [{},{}] and byte [{},{}) pair [{},{}] cross",
                    a_s, a_e, a_s, a_pe, b_s, b_e, b_s, b_pe);
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

        prop_assert!(covering.len() == min_cover,
            "greedy {} vs optimal {} (must be exact)", covering.len(), min_cover);
    }
}

fn brute_force_min_cover(intervals: &[(usize, usize)], num_pairs: usize) -> usize {
    if num_pairs == 0 {
        return 0;
    }
    // Filter to intervals that cover at least one pair
    let valid: Vec<_> = intervals
        .iter()
        .filter(|&&(s, e)| e > s + 1)
        .cloned()
        .collect();
    if valid.is_empty() {
        return usize::MAX;
    }

    // Greedy interval cover (optimal for interval covering on a line)
    let mut uncovered = 0;
    let mut count = 0;
    while uncovered < num_pairs {
        let best = valid
            .iter()
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
