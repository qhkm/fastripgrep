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
        let id = file_table.add(relative.as_os_str(), mtime, metadata.len());
        file_entries.push((id, path.clone()));
    }

    let file_count = file_entries.len() as u32;

    // Extract n-grams in parallel
    let per_file_ngrams: Vec<(u32, Vec<Vec<u8>>)> = file_entries
        .par_iter()
        .filter_map(|(id, path)| {
            let content = fs::read(path).ok()?;
            // Skip binary files (null byte in first 8KB)
            if crate::ignore::is_binary(&content) {
                return None;
            }
            // Always use build_all to guarantee recall. build_all is
            // O(n * MAX_NGRAM_LEN) per file thanks to the 64-byte cap
            // on inner loop length, so it is fast even for large files.
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
    let ngram_count = inverted.len() as u32;
    let meta = IndexMeta {
        version: 1,
        commit_hash,
        file_count,
        ngram_count,
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
    if let Some(ref h) = meta.commit_hash {
        println!("Git commit: {}", &h[..7.min(h.len())]);
    }
    Ok(())
}

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
