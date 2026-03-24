#!/usr/bin/env -S cargo +nightly -Zscript
//! Corpus-derived byte-pair weight table generator.
//!
//! Reads source code files and computes inverse-frequency weights for every
//! byte pair (a, b). The output is a Rust `const` array suitable for pasting
//! into `src/index/ngram.rs`.
//!
//! Usage:
//!   # Feed source files via stdin (one path per line):
//!   find /path/to/corpus -name '*.rs' | cargo +nightly -Zscript scripts/generate_weights.rs
//!
//!   # Or pass a directory argument:
//!   cargo +nightly -Zscript scripts/generate_weights.rs /path/to/corpus
//!
//! The current WEIGHT_TABLE in ngram.rs was derived from a simplified frequency
//! model (common English + code bigrams) rather than running this script against
//! a real corpus. This script is provided so that the table can be regenerated
//! from actual data in the future.

use std::env;
use std::fs;
use std::io::{self, BufRead, Read};
use std::path::{Path, PathBuf};

fn main() {
    let mut freq = [[0u64; 256]; 256];
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        // Directory mode: recursively scan source files
        let dir = &args[1];
        let extensions = ["rs", "py", "js", "ts", "c", "h", "cpp", "go", "java", "rb", "sh"];
        collect_files(Path::new(dir), &extensions, &mut |path| {
            if let Ok(data) = fs::read(path) {
                count_pairs(&data, &mut freq);
            }
        });
    } else {
        // Stdin mode: read file paths from stdin, or raw data if piped
        let stdin = io::stdin();
        let handle = stdin.lock();
        for line in handle.lines() {
            if let Ok(path) = line {
                let path = path.trim().to_string();
                if !path.is_empty() {
                    if let Ok(data) = fs::read(&path) {
                        count_pairs(&data, &mut freq);
                    }
                }
            }
        }
    }

    // Find max frequency
    let mut max_freq: u64 = 0;
    for a in 0..256 {
        for b in 0..256 {
            max_freq = max_freq.max(freq[a][b]);
        }
    }

    if max_freq == 0 {
        eprintln!("No data processed. Provide source files via stdin or directory argument.");
        std::process::exit(1);
    }

    // Compute inverse-frequency weights: weight = max_freq / (freq + 1), clamped to [1, 251]
    println!("pub static WEIGHT_TABLE: [[u32; 256]; 256] = [");
    for a in 0..256usize {
        print!("    [");
        for b in 0..256usize {
            let w = max_freq / (freq[a][b] + 1);
            let clamped = w.max(1).min(251) as u32;
            if b < 255 {
                print!("{},", clamped);
            } else {
                print!("{}", clamped);
            }
        }
        println!("],");
    }
    println!("];");

    // Print top-20 most common pairs for verification
    eprintln!("\n--- Top 20 most common byte pairs ---");
    let mut pairs: Vec<(u64, u8, u8)> = Vec::new();
    for a in 0..256u16 {
        for b in 0..256u16 {
            let f = freq[a as usize][b as usize];
            if f > 0 {
                pairs.push((f, a as u8, b as u8));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.cmp(&a.0));
    for (i, &(f, a, b)) in pairs.iter().take(20).enumerate() {
        let a_repr = if a.is_ascii_graphic() || a == b' ' {
            format!("'{}'", a as char)
        } else {
            format!("0x{:02x}", a)
        };
        let b_repr = if b.is_ascii_graphic() || b == b' ' {
            format!("'{}'", b as char)
        } else {
            format!("0x{:02x}", b)
        };
        eprintln!(
            "  {:2}. ({}, {}) freq={} weight={}",
            i + 1,
            a_repr,
            b_repr,
            f,
            (max_freq / (f + 1)).max(1).min(251)
        );
    }
}

fn count_pairs(data: &[u8], freq: &mut [[u64; 256]; 256]) {
    for window in data.windows(2) {
        freq[window[0] as usize][window[1] as usize] += 1;
    }
}

fn collect_files(dir: &Path, extensions: &[&str], callback: &mut dyn FnMut(&Path)) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden dirs and common non-source dirs
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "node_modules" || name == "target" || name == "vendor" {
                continue;
            }
            collect_files(&path, extensions, callback);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if extensions.contains(&ext) {
                callback(&path);
            }
        }
    }
}
