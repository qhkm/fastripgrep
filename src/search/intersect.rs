pub fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                result.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    result
}

pub fn sorted_union(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut result = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => {
                result.push(a[i]);
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                result.push(b[j]);
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                result.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    result.extend_from_slice(&a[i..]);
    result.extend_from_slice(&b[j..]);
    result
}

pub fn intersect_many(lists: &[Vec<u32>]) -> Vec<u32> {
    if lists.is_empty() {
        return Vec::new();
    }
    let mut r = lists[0].clone();
    for l in &lists[1..] {
        r = intersect(&r, l);
        if r.is_empty() {
            break;
        }
    }
    r
}

pub fn union_many(lists: &[Vec<u32>]) -> Vec<u32> {
    if lists.is_empty() {
        return Vec::new();
    }
    let mut r = lists[0].clone();
    for l in &lists[1..] {
        r = sorted_union(&r, l);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intersect() {
        assert_eq!(intersect(&[1, 3, 5, 7], &[3, 5, 9]), vec![3, 5]);
    }

    #[test]
    fn test_intersect_empty() {
        assert!(intersect(&[1, 2], &[]).is_empty());
    }

    #[test]
    fn test_intersect_disjoint() {
        assert!(intersect(&[1, 2], &[3, 4]).is_empty());
    }

    #[test]
    fn test_union() {
        assert_eq!(sorted_union(&[1, 3, 5], &[2, 3, 6]), vec![1, 2, 3, 5, 6]);
    }

    #[test]
    fn test_union_empty() {
        assert_eq!(sorted_union(&[1, 2], &[]), vec![1, 2]);
    }

    #[test]
    fn test_intersect_many() {
        let lists = vec![vec![1, 2, 3, 4], vec![2, 3, 4, 5], vec![3, 4, 5, 6]];
        assert_eq!(intersect_many(&lists), vec![3, 4]);
    }

    #[test]
    fn test_intersect_many_empty() {
        assert!(intersect_many(&[]).is_empty());
    }

    #[test]
    fn test_union_many() {
        let lists = vec![vec![1, 3], vec![2, 4], vec![3, 5]];
        assert_eq!(union_many(&lists), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_union_many_empty() {
        assert!(union_many(&[]).is_empty());
    }
}
