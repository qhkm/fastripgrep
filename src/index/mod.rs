pub mod filetable;
pub mod lookup;
pub mod meta;
pub mod ngram;
pub mod postings;

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::ignore::walk_files;
use filetable::{FileTableBuilder, FileTableReader};
use lookup::{write_lookup_table, LookupEntry};
use meta::IndexMeta;
use ngram::{build_all, hash_ngram};
use postings::encode_posting_list;

pub fn build_index(root: &Path, max_filesize: u64) -> Result<()> {
    build_index_follow(root, max_filesize, false)
}

pub fn build_index_follow(root: &Path, max_filesize: u64, follow_links: bool) -> Result<()> {
    let index_dir = root.join(".frg");
    fs::create_dir_all(&index_dir)?;

    // Acquire writer lock
    let lock_path = index_dir.join("lock");
    let lock_file = File::create(&lock_path)?;
    use fs4::fs_std::FileExt;
    lock_file.lock_exclusive()?;

    // Phase 1: Walk directory
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap());
    pb.set_message("Scanning files...");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    let files = walk_files(root, max_filesize, follow_links)?;
    pb.finish_and_clear();

    // Phase 2: Read files, filter binary, extract n-grams in parallel
    struct FileData {
        relative: std::path::PathBuf,
        mtime: u64,
        size: u64,
        ngrams: Vec<Vec<u8>>,
    }

    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(ProgressStyle::with_template(
        "{spinner:.cyan} Indexing [{bar:30.cyan/dim}] {pos}/{len} files ({per_sec})"
    ).unwrap().progress_chars("=> "));

    let file_data: Vec<FileData> = files
        .par_iter()
        .filter_map(|path| {
            let result = (|| {
                let content = fs::read(path).ok()?;
                if crate::ignore::is_binary(&content) {
                    return None;
                }
                let metadata = fs::metadata(path).ok()?;
                let mtime = metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
                let spans = build_all(&content);
                let ngrams: Vec<Vec<u8>> = spans.iter().map(|&(s, e)| content[s..e].to_vec()).collect();
                Some(FileData {
                    relative,
                    mtime,
                    size: metadata.len(),
                    ngrams,
                })
            })();
            pb.inc(1);
            result
        })
        .collect();
    pb.finish_and_clear();

    // Build file table and collect n-grams (sequential — assigns IDs)
    let mut file_table = FileTableBuilder::new();
    let mut per_file_ngrams: Vec<(u32, &Vec<Vec<u8>>)> = Vec::new();
    for fd in &file_data {
        let id = file_table.add(fd.relative.as_os_str(), fd.mtime, fd.size);
        per_file_ngrams.push((id, &fd.ngrams));
    }

    // Phase 3: Build inverted index
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap());
    pb.set_message("Building inverted index...");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    let mut inverted: HashMap<u64, Vec<u32>> = HashMap::new();
    for (file_id, ngrams) in &per_file_ngrams {
        let mut seen = std::collections::HashSet::new();
        for ngram_bytes in *ngrams {
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

    pb.finish_and_clear();

    // Phase 4: Write index
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap());
    pb.set_message("Writing index...");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    // Encode postings + build lookup
    let mut postings_data = Vec::new();
    let mut lookup_entries = Vec::new();
    for (hash, ids) in &inverted {
        let offset = postings_data.len() as u64;
        let encoded = encode_posting_list(ids);
        let length = encoded.len() as u32;
        postings_data.extend_from_slice(&encoded);
        lookup_entries.push(LookupEntry {
            hash: *hash,
            offset,
            length,
        });
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
        file_count: file_table.len() as u32,
        ngram_count,
        timestamp: IndexMeta::timestamp_now(),
        overlay_file_count: 0,
        overlay_ngram_count: 0,
        tombstone_count: 0,
    };
    meta.write(&gen_dir.join("meta.json"))?;

    // Atomically switch CURRENT
    let tmp = index_dir.join("CURRENT.tmp");
    fs::write(&tmp, &gen_id)?;
    fs::rename(&tmp, index_dir.join("CURRENT"))?;
    pb.finish_and_clear();

    // Release lock (dropped automatically, but explicit)
    lock_file.unlock()?;

    Ok(())
}

/// Write tombstones as a sorted list of u32 file IDs, 4 bytes each (little-endian).
pub fn write_tombstones(path: &Path, tombstones: &[u32]) -> Result<()> {
    let mut sorted = tombstones.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut buf = Vec::with_capacity(sorted.len() * 4);
    for id in &sorted {
        buf.extend_from_slice(&id.to_le_bytes());
    }
    fs::write(path, &buf)?;
    Ok(())
}

/// Read tombstones from a file (sorted list of u32 file IDs, 4 bytes each).
pub fn read_tombstones(path: &Path) -> Result<Vec<u32>> {
    let data = fs::read(path)?;
    if data.len() % 4 != 0 {
        anyhow::bail!("tombstones.bin has invalid length");
    }
    let mut ids = Vec::with_capacity(data.len() / 4);
    for chunk in data.chunks_exact(4) {
        ids.push(u32::from_le_bytes(chunk.try_into().unwrap()));
    }
    Ok(ids)
}

/// Check if a file ID is in the tombstone set (binary search on sorted list).
pub fn is_tombstoned(tombstones: &[u32], id: u32) -> bool {
    tombstones.binary_search(&id).is_ok()
}

pub fn update_index(root: &Path, max_filesize: u64) -> Result<()> {
    update_index_follow(root, max_filesize, false)
}

pub fn update_index_follow(root: &Path, max_filesize: u64, follow_links: bool) -> Result<()> {
    // Check if a base generation exists; if not, fall back to full build
    let gen_dir = match current_generation(root) {
        Ok(dir) if dir.join("meta.json").exists() => dir,
        _ => return build_index_follow(root, max_filesize, follow_links),
    };

    let index_dir = root.join(".frg");

    // Acquire writer lock
    let lock_path = index_dir.join("lock");
    let lock_file = File::create(&lock_path)?;
    use fs4::fs_std::FileExt;
    lock_file.lock_exclusive()?;

    // Load base file table
    let base_ft = FileTableReader::open(&gen_dir.join("files.bin"))?;
    let base_file_count = base_ft.len() as u32;

    // Build a map of relative_path -> (base_id, mtime, size) from the base
    let mut base_map: HashMap<std::path::PathBuf, (u32, u64, u64)> = HashMap::new();
    for id in 0..base_file_count {
        if let Some(entry) = base_ft.get(id) {
            base_map.insert(entry.path.clone(), (id, entry.mtime, entry.size));
        }
    }

    // Walk current directory
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap());
    pb.set_message("Scanning files...");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    let current_files = walk_files(root, max_filesize, follow_links)?;
    pb.finish_and_clear();

    // Build set of current relative paths
    let mut current_relative: HashMap<std::path::PathBuf, &std::path::Path> = HashMap::new();
    for path in &current_files {
        let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        current_relative.insert(relative, path.as_path());
    }

    // Classify files
    let mut tombstones: Vec<u32> = Vec::new();
    let mut files_to_index: Vec<std::path::PathBuf> = Vec::new(); // absolute paths of changed/new files

    // Check base files: deleted or changed?
    for (rel_path, (base_id, base_mtime, base_size)) in &base_map {
        if let Some(abs_path) = current_relative.get(rel_path) {
            // File still exists — check if changed
            if let Ok(metadata) = fs::metadata(abs_path) {
                let mtime = metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let size = metadata.len();
                if mtime != *base_mtime || size != *base_size {
                    // Changed: tombstone base, add to overlay
                    tombstones.push(*base_id);
                    files_to_index.push(abs_path.to_path_buf());
                }
                // else: unchanged, skip
            }
        } else {
            // Deleted: tombstone the base ID
            tombstones.push(*base_id);
        }
    }

    // Check for new files (in current but not in base)
    for (rel_path, abs_path) in &current_relative {
        if !base_map.contains_key(rel_path) {
            files_to_index.push(abs_path.to_path_buf());
        }
    }

    // If no changes, write minimal overlay and update meta
    if files_to_index.is_empty() && tombstones.is_empty() {
        // Remove any previous overlay
        let overlay_dir = gen_dir.join("overlay");
        if overlay_dir.exists() {
            fs::remove_dir_all(&overlay_dir)?;
        }
        // Update meta timestamp
        let mut meta = IndexMeta::read(&gen_dir.join("meta.json"))?;
        meta.timestamp = IndexMeta::timestamp_now();
        meta.overlay_file_count = 0;
        meta.overlay_ngram_count = 0;
        meta.tombstone_count = 0;
        meta.write(&gen_dir.join("meta.json"))?;
        lock_file.unlock()?;
        return Ok(());
    }

    // Phase 2: Read files, filter binary, extract n-grams in parallel
    struct FileData {
        relative: std::path::PathBuf,
        mtime: u64,
        size: u64,
        ngrams: Vec<Vec<u8>>,
    }

    let pb = ProgressBar::new(files_to_index.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.cyan} Indexing overlay [{bar:30.cyan/dim}] {pos}/{len} files ({per_sec})",
        )
        .unwrap()
        .progress_chars("=> "),
    );

    let file_data: Vec<FileData> = files_to_index
        .par_iter()
        .filter_map(|path| {
            let result = (|| {
                let content = fs::read(path).ok()?;
                if crate::ignore::is_binary(&content) {
                    return None;
                }
                let metadata = fs::metadata(path).ok()?;
                let mtime = metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
                let spans = build_all(&content);
                let ngrams: Vec<Vec<u8>> =
                    spans.iter().map(|&(s, e)| content[s..e].to_vec()).collect();
                Some(FileData {
                    relative,
                    mtime,
                    size: metadata.len(),
                    ngrams,
                })
            })();
            pb.inc(1);
            result
        })
        .collect();
    pb.finish_and_clear();

    // Build overlay file table with IDs starting at base_file_count
    let mut overlay_ft = FileTableBuilder::new();
    let mut per_file_ngrams: Vec<(u32, &Vec<Vec<u8>>)> = Vec::new();
    for fd in &file_data {
        let local_id = overlay_ft.add(fd.relative.as_os_str(), fd.mtime, fd.size);
        let global_id = base_file_count + local_id;
        per_file_ngrams.push((global_id, &fd.ngrams));
    }

    // Build overlay inverted index
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap());
    pb.set_message("Building overlay inverted index...");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    let mut inverted: HashMap<u64, Vec<u32>> = HashMap::new();
    for (file_id, ngrams) in &per_file_ngrams {
        let mut seen = std::collections::HashSet::new();
        for ngram_bytes in *ngrams {
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
    pb.finish_and_clear();

    // Write overlay
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.cyan} {msg}").unwrap());
    pb.set_message("Writing overlay...");
    pb.enable_steady_tick(std::time::Duration::from_millis(80));

    // Encode overlay postings + build lookup
    let mut postings_data = Vec::new();
    let mut lookup_entries = Vec::new();
    for (hash, ids) in &inverted {
        let offset = postings_data.len() as u64;
        let encoded = encode_posting_list(ids);
        let length = encoded.len() as u32;
        postings_data.extend_from_slice(&encoded);
        lookup_entries.push(LookupEntry {
            hash: *hash,
            offset,
            length,
        });
    }

    // Write to temp overlay dir, then rename atomically
    let overlay_dir = gen_dir.join("overlay");
    let overlay_tmp = gen_dir.join("overlay.tmp");
    if overlay_tmp.exists() {
        fs::remove_dir_all(&overlay_tmp)?;
    }
    fs::create_dir_all(&overlay_tmp)?;

    let mut pf = File::create(overlay_tmp.join("postings.bin"))?;
    pf.write_all(&postings_data)?;
    let mut lf = File::create(overlay_tmp.join("lookup.bin"))?;
    write_lookup_table(&lookup_entries, &mut lf)?;
    let mut ff = File::create(overlay_tmp.join("files.bin"))?;
    overlay_ft.write(&mut ff)?;
    write_tombstones(&overlay_tmp.join("tombstones.bin"), &tombstones)?;

    // Atomically replace overlay
    if overlay_dir.exists() {
        fs::remove_dir_all(&overlay_dir)?;
    }
    fs::rename(&overlay_tmp, &overlay_dir)?;

    // Update meta
    let mut meta = IndexMeta::read(&gen_dir.join("meta.json"))?;
    meta.timestamp = IndexMeta::timestamp_now();
    meta.overlay_file_count = overlay_ft.len() as u32;
    meta.overlay_ngram_count = inverted.len() as u32;
    meta.tombstone_count = tombstones.len() as u32;
    meta.write(&gen_dir.join("meta.json"))?;

    pb.finish_and_clear();
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
    let index_dir = root.join(".frg");
    let current = fs::read_to_string(index_dir.join("CURRENT"))?
        .trim()
        .to_string();
    Ok(index_dir.join("generations").join(current))
}

pub fn index_status(root: &Path) -> Result<()> {
    let gen_dir = current_generation(root)?;
    let meta = IndexMeta::read(&gen_dir.join("meta.json"))?;
    let age = IndexMeta::timestamp_now().saturating_sub(meta.timestamp);
    let age_str = if age < 60 {
        format!("{}s ago", age)
    } else if age < 3600 {
        format!("{}m ago", age / 60)
    } else if age < 86400 {
        format!("{}h ago", age / 3600)
    } else {
        format!("{}d ago", age / 86400)
    };
    println!("Index version: {}", meta.version);
    if meta.overlay_file_count > 0 || meta.tombstone_count > 0 {
        println!(
            "Files indexed: {} (base) + {} (overlay), {} tombstoned",
            meta.file_count, meta.overlay_file_count, meta.tombstone_count
        );
    } else {
        println!("Files indexed: {}", meta.file_count);
    }
    println!("Unique n-grams: {}", meta.ngram_count);
    if meta.overlay_ngram_count > 0 {
        println!("Overlay n-grams: {}", meta.overlay_ngram_count);
    }
    println!("Built: {}", age_str);
    if let Some(ref h) = meta.commit_hash {
        println!("Git commit: {}", &h[..7.min(h.len())]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_build_index() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.rs"), "fn hello_world() {}").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() { hello_world(); }").unwrap();
        let result = build_index(dir.path(), 10 * 1024 * 1024);
        assert!(result.is_ok());
        let index_dir = dir.path().join(".frg");
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

    #[test]
    fn test_update_index_detects_changes() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.rs"), "fn hello_world() {}").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() { hello_world(); }").unwrap();
        build_index(dir.path(), 10 * 1024 * 1024).unwrap();

        // Search for original content
        let opts = crate::search::SearchOptions::default();
        let r = crate::search::search(dir.path(), "hello_world", &opts).unwrap();
        assert!(!r.is_empty(), "should find hello_world before update");

        // Modify a file — change content
        // Need to ensure mtime changes (some filesystems have 1s granularity)
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(dir.path().join("hello.rs"), "fn goodbye_world() {}").unwrap();

        // Run incremental update
        update_index(dir.path(), 10 * 1024 * 1024).unwrap();

        // Verify overlay was created
        let gen_dir = current_generation(dir.path()).unwrap();
        assert!(gen_dir.join("overlay").exists(), "overlay directory should exist");
        let meta = IndexMeta::read(&gen_dir.join("meta.json")).unwrap();
        assert!(meta.overlay_file_count > 0, "should have overlay files");
        assert!(meta.tombstone_count > 0, "should have tombstoned the old file");

        // Search for new content
        let r = crate::search::search(dir.path(), "goodbye_world", &opts).unwrap();
        assert!(!r.is_empty(), "should find goodbye_world after update");
    }

    #[test]
    fn test_update_index_new_file() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.rs"), "fn hello_world() {}").unwrap();
        build_index(dir.path(), 10 * 1024 * 1024).unwrap();

        // Add a new file
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(
            dir.path().join("newfile.rs"),
            "fn brand_new_function() { unique_content_xyz123 }",
        )
        .unwrap();

        // Run incremental update
        update_index(dir.path(), 10 * 1024 * 1024).unwrap();

        // Verify overlay was created
        let gen_dir = current_generation(dir.path()).unwrap();
        assert!(gen_dir.join("overlay").exists(), "overlay directory should exist");
        let meta = IndexMeta::read(&gen_dir.join("meta.json")).unwrap();
        assert!(meta.overlay_file_count > 0, "should have overlay files for new file");
        assert_eq!(meta.tombstone_count, 0, "no files were modified/deleted");

        // Search for new file content
        let opts = crate::search::SearchOptions::default();
        let r = crate::search::search(dir.path(), "brand_new_function", &opts).unwrap();
        assert!(!r.is_empty(), "should find new file content after update");
    }

    #[test]
    fn test_update_index_deleted_file() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.rs"), "fn hello_world() {}").unwrap();
        fs::write(
            dir.path().join("delete_me.rs"),
            "fn unique_delete_target_xyz() {}",
        )
        .unwrap();
        build_index(dir.path(), 10 * 1024 * 1024).unwrap();

        // Verify the file is found before deletion
        let opts = crate::search::SearchOptions::default();
        let r = crate::search::search(dir.path(), "unique_delete_target_xyz", &opts).unwrap();
        assert!(!r.is_empty(), "should find content before deletion");

        // Delete the file
        fs::remove_file(dir.path().join("delete_me.rs")).unwrap();

        // Run incremental update
        update_index(dir.path(), 10 * 1024 * 1024).unwrap();

        // Verify tombstone was created
        let gen_dir = current_generation(dir.path()).unwrap();
        let meta = IndexMeta::read(&gen_dir.join("meta.json")).unwrap();
        assert!(meta.tombstone_count > 0, "should have tombstoned the deleted file");

        // Search should NOT find the deleted file's content
        let r = crate::search::search(dir.path(), "unique_delete_target_xyz", &opts).unwrap();
        assert!(r.is_empty(), "should not find deleted file content after update");
    }

    #[test]
    fn test_update_no_changes() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("hello.rs"), "fn hello_world() {}").unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() { hello_world(); }").unwrap();
        build_index(dir.path(), 10 * 1024 * 1024).unwrap();

        // Run update with no changes
        update_index(dir.path(), 10 * 1024 * 1024).unwrap();

        // Verify no overlay was created
        let gen_dir = current_generation(dir.path()).unwrap();
        let meta = IndexMeta::read(&gen_dir.join("meta.json")).unwrap();
        assert_eq!(meta.overlay_file_count, 0, "no overlay files when nothing changed");
        assert_eq!(meta.tombstone_count, 0, "no tombstones when nothing changed");
        assert!(!gen_dir.join("overlay").exists(), "overlay dir should not exist when nothing changed");

        // Search should still work
        let opts = crate::search::SearchOptions::default();
        let r = crate::search::search(dir.path(), "hello_world", &opts).unwrap();
        assert!(!r.is_empty(), "search should still work after no-change update");
    }
}
