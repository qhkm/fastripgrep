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

/// Build a multi-line version of a regex for whole-file early bail.
/// This ensures ^ and $ match at line boundaries, not just start/end of content.
fn multiline_regex(re: &Regex) -> Option<Regex> {
    let pattern = format!("(?m){}", re.as_str());
    Regex::new(&pattern).ok()
}

pub fn verify_file(path: &Path, re: &Regex, max_count: Option<usize>, context: usize) -> Vec<Match> {
    let content = match std::fs::read(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let file_path = path.to_string_lossy().into_owned();

    // Early bail: check whole file for any match before splitting into lines.
    // Use (?m) multi-line mode so ^ and $ match at line boundaries.
    let bail_re = multiline_regex(re).unwrap_or_else(|| re.clone());
    if !bail_re.is_match(&content) {
        return Vec::new();
    }

    let lines: Vec<&[u8]> = content.split(|&b| b == b'\n').collect();
    let mut matches = Vec::new();

    for (line_idx, line) in lines.iter().enumerate() {
        if let Some(max) = max_count {
            if matches.len() >= max {
                break;
            }
        }
        if let Some(m) = re.find(line) {
            let mut ctx_before = Vec::new();
            let mut ctx_after = Vec::new();

            if context > 0 {
                let start = line_idx.saturating_sub(context);
                for (ci, line) in lines.iter().enumerate().take(line_idx).skip(start) {
                    ctx_before.push((ci + 1, String::from_utf8_lossy(line).into_owned()));
                }
                let end = (line_idx + 1 + context).min(lines.len());
                for (ci, line) in lines.iter().enumerate().take(end).skip(line_idx + 1) {
                    ctx_after.push((ci + 1, String::from_utf8_lossy(line).into_owned()));
                }
            }

            matches.push(Match {
                file_path: file_path.clone(),
                line_number: line_idx + 1,
                line_content: String::from_utf8_lossy(line).into_owned(),
                match_start: m.start(),
                match_end: m.end(),
                context_before: ctx_before,
                context_after: ctx_after,
            });
        }
    }
    matches
}

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
        let re = Regex::new("hello_world").unwrap();
        let matches = verify_file(&path, &re, None, 0);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_number, 1);
    }

    #[test]
    fn test_verify_file_no_match() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.rs");
        fs::write(&path, "fn main() {}").unwrap();
        let re = Regex::new("nonexistent").unwrap();
        assert!(verify_file(&path, &re, None, 0).is_empty());
    }

    #[test]
    fn test_verify_with_context() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.rs");
        fs::write(&path, "line1\nline2\nmatch_here\nline4\nline5\n").unwrap();
        let re = Regex::new("match_here").unwrap();
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
        let re = Regex::new("aaa").unwrap();
        let matches = verify_file(&path, &re, Some(2), 0);
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_verify_context_at_start() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.rs");
        fs::write(&path, "match_here\nline2\nline3\n").unwrap();
        let re = Regex::new("match_here").unwrap();
        let matches = verify_file(&path, &re, None, 2);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].context_before.len(), 0); // no lines before
        assert_eq!(matches[0].context_after.len(), 2);
    }

    #[test]
    fn test_verify_match_offsets() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.rs");
        fs::write(&path, "fn hello() {}").unwrap();
        let re = Regex::new("hello").unwrap();
        let matches = verify_file(&path, &re, None, 0);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].match_start, 3);
        assert_eq!(matches[0].match_end, 8);
    }

    #[test]
    fn test_verify_nonexistent_file() {
        let re = Regex::new("hello").unwrap();
        let matches = verify_file(Path::new("/nonexistent/path.rs"), &re, None, 0);
        assert!(matches.is_empty());
    }
}
