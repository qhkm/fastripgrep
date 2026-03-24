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

    #[test]
    fn test_plan_alternation_with_short_branch() {
        // "a|foo" - branch "a" is < 2 bytes, so ScanAll propagates
        let plan = build_query_plan("a|foo");
        assert!(matches!(plan, QueryPlan::ScanAll),
            "alternation with 1-byte branch should be ScanAll");
    }

    #[test]
    fn test_plan_two_byte_literal() {
        let plan = build_query_plan("ab");
        assert!(!matches!(plan, QueryPlan::ScanAll),
            "two-byte literal should produce a Lookup");
    }
}
