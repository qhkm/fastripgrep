pub mod decompose;
pub mod intersect;
pub mod verify;

use crate::index::filetable::FileTableReader;
use crate::index::lookup::MmapLookupTable;
use crate::index::postings::decode_posting_list;
use crate::index::{current_generation, meta::IndexMeta};
use anyhow::Result;
use decompose::{build_query_plan, QueryPlan};
use intersect::{intersect_many, union_many};
use std::io::Write;
use std::path::Path;
use verify::{verify_file, Match};

#[derive(Debug, Clone, Default)]
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

pub fn search(root: &Path, pattern: &str, opts: &SearchOptions) -> Result<Vec<Match>> {
    let effective = if opts.literal {
        regex::escape(pattern)
    } else {
        pattern.to_string()
    };
    let effective = if opts.case_insensitive {
        format!("(?i){}", effective)
    } else {
        effective
    };

    let re = regex::bytes::Regex::new(&effective)?;

    if opts.no_index {
        return brute_force_search(root, &re, opts);
    }

    let gen_dir = current_generation(root)?;
    let meta = IndexMeta::read(&gen_dir.join("meta.json"))?;
    let age = IndexMeta::timestamp_now().saturating_sub(meta.timestamp);
    if age > 86400 {
        eprintln!(
            "warning: index is {}h old, consider `rsgrep update`",
            age / 3600
        );
    }

    let lookup = MmapLookupTable::open(&gen_dir.join("lookup.bin"))?;
    let postings_data = std::fs::read(gen_dir.join("postings.bin"))?;
    let file_table = FileTableReader::open(&gen_dir.join("files.bin"))?;

    let plan = build_query_plan(&effective);
    let candidates = execute_plan(&plan, &lookup, &postings_data, &file_table)?;

    // Collect candidate paths, filtering first
    let candidate_paths: Vec<_> = candidates
        .iter()
        .filter_map(|fid| {
            let entry = file_table.get(*fid)?;
            let full_path = root.join(&entry.path);
            if matches_filters(&full_path, opts) {
                Some(full_path)
            } else {
                None
            }
        })
        .collect();

    // Verify in parallel with rayon
    use rayon::prelude::*;
    let all_matches: Vec<Match> = candidate_paths
        .par_iter()
        .flat_map(|path| verify_file(path, &re, opts.max_count, opts.context))
        .collect();

    Ok(all_matches)
}

pub fn brute_force_search(
    root: &Path,
    re: &regex::bytes::Regex,
    opts: &SearchOptions,
) -> Result<Vec<Match>> {
    let files = crate::ignore::walk_files(root, 10 * 1024 * 1024)?;
    let mut all = Vec::new();
    for path in &files {
        if !matches_filters(path, opts) {
            continue;
        }
        let m = verify_file(path, re, opts.max_count, opts.context);
        all.extend(m);
    }
    Ok(all)
}

fn matches_filters(path: &Path, opts: &SearchOptions) -> bool {
    if let Some(ref glob_pat) = opts.glob_pattern {
        if let Ok(pat) = glob::Pattern::new(glob_pat) {
            if !pat.matches_path(path) {
                return false;
            }
        }
    }
    if let Some(ref ext) = opts.file_type {
        if path.extension().and_then(|e| e.to_str()) != Some(ext.as_str()) {
            return false;
        }
    }
    true
}

/// Classify a pattern for fast-path optimization.
#[derive(Debug, PartialEq)]
pub enum PatternKind {
    /// Matches every line (e.g., "", ".*", ".+")
    MatchAll,
    /// Single byte literal — use memchr for SIMD search
    SingleByte(u8),
    /// Normal regex — use standard path
    Normal,
}

pub fn classify_pattern(pattern: &str) -> PatternKind {
    match pattern {
        "" | ".*" | ".+" | "^.*$" | "^.+$" => PatternKind::MatchAll,
        _ if pattern.len() == 1 && pattern.as_bytes()[0].is_ascii_graphic() => {
            PatternKind::SingleByte(pattern.as_bytes()[0])
        }
        _ => PatternKind::Normal,
    }
}

/// Streaming search: processes files in parallel, writes output in file order.
/// Much faster for ScanAll patterns with millions of matches.
pub fn search_streaming<W: Write>(
    root: &Path,
    pattern: &str,
    opts: &SearchOptions,
    writer: &mut W,
    use_color: bool,
) -> Result<u64> {
    use rayon::prelude::*;

    let effective = if opts.literal {
        regex::escape(pattern)
    } else {
        pattern.to_string()
    };
    let effective = if opts.case_insensitive {
        format!("(?i){}", effective)
    } else {
        effective
    };

    let kind = classify_pattern(&effective);

    // Get file list
    let file_paths: Vec<std::path::PathBuf> = if opts.no_index {
        crate::ignore::walk_files(root, 10 * 1024 * 1024)?
    } else {
        let gen_dir = current_generation(root)?;
        let meta = IndexMeta::read(&gen_dir.join("meta.json"))?;
        let age = IndexMeta::timestamp_now().saturating_sub(meta.timestamp);
        if age > 86400 {
            eprintln!("warning: index is {}h old, consider `rsgrep update`", age / 3600);
        }

        let lookup = MmapLookupTable::open(&gen_dir.join("lookup.bin"))?;
        let postings_data = std::fs::read(gen_dir.join("postings.bin"))?;
        let file_table = FileTableReader::open(&gen_dir.join("files.bin"))?;

        let plan = build_query_plan(&effective);
        let candidates = execute_plan(&plan, &lookup, &postings_data, &file_table)?;

        candidates.iter()
            .filter_map(|fid| {
                let entry = file_table.get(*fid)?;
                Some(root.join(&entry.path))
            })
            .collect()
    };

    // Filter paths first
    let filtered: Vec<_> = file_paths.iter()
        .filter(|p| matches_filters(p, opts))
        .collect();

    let suppress_output = opts.quiet || opts.files_only || opts.count;

    // Process files in parallel — each produces (match_count, output_buffer)
    let results: Vec<(u64, Vec<u8>)> = filtered
        .par_iter()
        .filter_map(|path| {
            let content = std::fs::read(path).ok()?;
            if crate::ignore::is_binary(&content) {
                return None;
            }

            let file_str = path.to_string_lossy();
            let mut buf = Vec::new();
            let mut count: u64 = 0;

            match kind {
                PatternKind::MatchAll => {
                    let lines: Vec<&[u8]> = content.split(|&b| b == b'\n').collect();
                    let n = if lines.last().is_some_and(|l| l.is_empty()) {
                        lines.len() - 1
                    } else {
                        lines.len()
                    };
                    for (idx, line) in lines[..n].iter().enumerate() {
                        if let Some(max) = opts.max_count {
                            if count >= max as u64 { break; }
                        }
                        count += 1;
                        if !suppress_output {
                            if use_color {
                                let _ = writeln!(buf, "\x1b[35m{}\x1b[0m:\x1b[32m{}\x1b[0m:{}",
                                    file_str, idx + 1, String::from_utf8_lossy(line));
                            } else {
                                // Write raw bytes directly — avoid from_utf8_lossy for ASCII
                                let _ = write!(buf, "{}:{}:", file_str, idx + 1);
                                buf.extend_from_slice(line);
                                buf.push(b'\n');
                            }
                        }
                    }
                }
                PatternKind::SingleByte(byte) => {
                    for (idx, line) in content.split(|&b| b == b'\n').enumerate() {
                        if let Some(max) = opts.max_count {
                            if count >= max as u64 { break; }
                        }
                        if memchr::memchr(byte, line).is_some() {
                            count += 1;
                            if !suppress_output {
                                if use_color {
                                    let _ = writeln!(buf, "\x1b[35m{}\x1b[0m:\x1b[32m{}\x1b[0m:{}",
                                        file_str, idx + 1, String::from_utf8_lossy(line));
                                } else {
                                    let _ = write!(buf, "{}:{}:", file_str, idx + 1);
                                    buf.extend_from_slice(line);
                                    buf.push(b'\n');
                                }
                            }
                        }
                    }
                }
                PatternKind::Normal => unreachable!(),
            }

            if count > 0 { Some((count, buf)) } else { None }
        })
        .collect();

    // Write output buffers sequentially (maintains file order from parallel processing)
    let mut total: u64 = 0;
    for (count, buf) in &results {
        total += count;
        if !buf.is_empty() {
            let _ = writer.write_all(buf);
        }
    }

    Ok(total)
}

fn execute_plan(
    plan: &QueryPlan,
    lookup: &MmapLookupTable,
    postings: &[u8],
    ft: &FileTableReader,
) -> Result<Vec<u32>> {
    match plan {
        QueryPlan::Lookup(hash) => {
            if let Some((offset, length)) = lookup.lookup(*hash) {
                let s = offset as usize;
                let e = s + length as usize;
                if e <= postings.len() {
                    Ok(decode_posting_list(&postings[s..e]))
                } else {
                    Ok(Vec::new())
                }
            } else {
                Ok(Vec::new())
            }
        }
        QueryPlan::And(subs) => {
            let lists: Result<Vec<_>> = subs
                .iter()
                .map(|s| execute_plan(s, lookup, postings, ft))
                .collect();
            Ok(intersect_many(&lists?))
        }
        QueryPlan::Or(subs) => {
            let lists: Result<Vec<_>> = subs
                .iter()
                .map(|s| execute_plan(s, lookup, postings, ft))
                .collect();
            Ok(union_many(&lists?))
        }
        QueryPlan::ScanAll => Ok(ft.all_file_ids()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("hello.rs"),
            "fn hello_world() {\n    println!(\"hi\");\n}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("main.rs"),
            "fn main() {\n    hello_world();\n}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("other.rs"),
            "fn other() {\n    let x = 42;\n}\n",
        )
        .unwrap();
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
        assert!(
            r.len() >= 3,
            "alternation should find matches from both branches"
        );
    }

    #[test]
    fn test_search_brute_force() {
        let dir = setup();
        let opts = SearchOptions {
            no_index: true,
            ..Default::default()
        };
        let r = search(dir.path(), "hello_world", &opts).unwrap();
        assert!(r.len() >= 2);
    }

    #[test]
    fn test_search_case_insensitive() {
        let dir = setup();
        let opts = SearchOptions {
            case_insensitive: true,
            ..Default::default()
        };
        let r = search(dir.path(), "HELLO_WORLD", &opts).unwrap();
        assert!(!r.is_empty());
    }

    #[test]
    fn test_search_literal() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("test.rs"), "let x = a.b();").unwrap();
        crate::index::build_index(dir.path(), 10 * 1024 * 1024).unwrap();
        let opts = SearchOptions {
            literal: true,
            ..Default::default()
        };
        let r = search(dir.path(), "a.b()", &opts).unwrap();
        assert!(!r.is_empty());
    }

    #[test]
    fn test_search_glob_filter() {
        let dir = setup();
        let opts = SearchOptions {
            glob_pattern: Some("*.rs".to_string()),
            no_index: true,
            ..Default::default()
        };
        let r = search(dir.path(), "fn", &opts).unwrap();
        assert!(!r.is_empty());
    }
}
