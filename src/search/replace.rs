use anyhow::Result;
use std::path::Path;

pub struct ReplaceResult {
    pub file_path: String,
    pub replacements: usize,
    pub original_lines: Vec<(usize, String)>,  // (line_num, content)
    pub replaced_lines: Vec<(usize, String)>,  // (line_num, content)
}

pub fn replace_in_file(
    path: &Path,
    re: &regex::bytes::Regex,
    replacement: &[u8],
) -> Option<ReplaceResult> {
    let content = std::fs::read(path).ok()?;
    if crate::ignore::is_binary(&content) {
        return None;
    }

    let replaced = re.replace_all(&content, replacement);
    if replaced.as_ref() == content.as_slice() {
        return None; // no changes
    }

    let file_path = path.to_string_lossy().to_string();
    let orig_lines: Vec<&[u8]> = content.split(|&b| b == b'\n').collect();
    let new_lines: Vec<&[u8]> = replaced.split(|&b| b == b'\n').collect();

    let mut original = Vec::new();
    let mut replaced_out = Vec::new();
    let mut count = 0;

    let max_lines = orig_lines.len().max(new_lines.len());
    for i in 0..max_lines {
        let orig = orig_lines.get(i).copied().unwrap_or(b"");
        let repl = new_lines.get(i).copied().unwrap_or(b"");
        if orig != repl {
            count += 1;
            original.push((i + 1, String::from_utf8_lossy(orig).into_owned()));
            replaced_out.push((i + 1, String::from_utf8_lossy(repl).into_owned()));
        }
    }

    if count == 0 {
        return None;
    }

    Some(ReplaceResult {
        file_path,
        replacements: count,
        original_lines: original,
        replaced_lines: replaced_out,
    })
}

pub fn write_replacement(path: &Path, re: &regex::bytes::Regex, replacement: &[u8]) -> Result<usize> {
    let content = std::fs::read(path)?;
    let replaced = re.replace_all(&content, replacement);
    if replaced.as_ref() == content.as_slice() {
        return Ok(0);
    }
    let count = re.find_iter(&content).count();
    std::fs::write(path, replaced.as_ref())?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_replace_in_file_basic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "hello world\ngoodbye world\n").unwrap();
        let re = regex::bytes::Regex::new("world").unwrap();
        let result = replace_in_file(&path, &re, b"earth").unwrap();
        assert_eq!(result.replacements, 2);
        assert_eq!(result.original_lines.len(), 2);
        assert_eq!(result.replaced_lines[0].1, "hello earth");
        assert_eq!(result.replaced_lines[1].1, "goodbye earth");
    }

    #[test]
    fn test_replace_in_file_no_match() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "hello world\n").unwrap();
        let re = regex::bytes::Regex::new("nonexistent").unwrap();
        assert!(replace_in_file(&path, &re, b"replacement").is_none());
    }

    #[test]
    fn test_write_replacement() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "foo bar foo\n").unwrap();
        let re = regex::bytes::Regex::new("foo").unwrap();
        let count = write_replacement(&path, &re, b"baz").unwrap();
        assert_eq!(count, 2);
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "baz bar baz\n");
    }

    #[test]
    fn test_write_replacement_no_match() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "hello world\n").unwrap();
        let re = regex::bytes::Regex::new("nonexistent").unwrap();
        let count = write_replacement(&path, &re, b"replacement").unwrap();
        assert_eq!(count, 0);
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello world\n");
    }

    #[test]
    fn test_replace_with_capture_groups() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "fn hello() {}\nfn world() {}\n").unwrap();
        let re = regex::bytes::Regex::new(r"fn (\w+)\(\)").unwrap();
        let result = replace_in_file(&path, &re, b"fn renamed_$1()").unwrap();
        assert_eq!(result.replacements, 2);
        assert_eq!(result.replaced_lines[0].1, "fn renamed_hello() {}");
        assert_eq!(result.replaced_lines[1].1, "fn renamed_world() {}");
    }
}
