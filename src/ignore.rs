use anyhow::Result;
use ignore::WalkBuilder;
use std::fs;
use std::path::{Path, PathBuf};

pub fn is_binary(content: &[u8]) -> bool {
    let check_len = content.len().min(8192);
    content[..check_len].contains(&0)
}

/// Walk a directory collecting file paths. Respects .gitignore and .frgignore.
/// Skips files over max_filesize. Does NOT check binary here — caller should
/// check after reading content to avoid double-reading files.
pub fn walk_files(root: &Path, max_filesize: u64, follow_links: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let walker = WalkBuilder::new(root)
        .add_custom_ignore_filename(".frgignore")
        .follow_links(follow_links)
        .require_git(false)
        .build();

    for entry in walker {
        let entry = entry?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > max_filesize {
                continue;
            }
        }
        files.push(path.to_path_buf());
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
        let files = walk_files(dir.path(), 10 * 1024 * 1024, false).unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"keep.rs"));
        assert!(!names.contains(&"skip.log"));
    }

    #[test]
    fn test_walk_includes_all_non_ignored() {
        // Binary detection is no longer in the walker — it's done by the indexer/verifier.
        // Walker should include all files that aren't gitignored or over size limit.
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("code.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("bin.dat"), b"\x00\x01\x02\x03").unwrap();
        let files = walk_files(dir.path(), 10 * 1024 * 1024, false).unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"code.rs"));
        assert!(names.contains(&"bin.dat")); // walker includes it; indexer filters it
    }
}
