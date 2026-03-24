use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use std::io::Write;
use std::path::PathBuf;
use std::process;

use crate::index;
use crate::output;
use crate::search::{self, SearchOptions};

/// Load config file args from `~/.frgrc` or `$FRG_CONFIG_PATH`.
/// Each line is one argument (like ripgrep's config format).
/// Lines starting with `#` are comments. Empty lines are ignored.
fn load_config_args() -> Vec<String> {
    let config_path = std::env::var("FRG_CONFIG_PATH")
        .map(PathBuf::from)
        .ok()
        .or_else(|| dirs_next().map(|home| home.join(".frgrc")));

    let path = match config_path {
        Some(p) if p.exists() => p,
        _ => return Vec::new(),
    };

    match std::fs::read_to_string(&path) {
        Ok(content) => content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

#[derive(Parser)]
#[command(
    name = "frg",
    version,
    about = "Fast regex search with sparse n-gram indexing"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build or rebuild the search index
    Index {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "10485760")]
        max_filesize: u64,
        /// Follow symbolic links
        #[arg(long)]
        follow: bool,
    },
    /// Search using the index
    Search {
        pattern: String,
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short = 'n', long)]
        no_index: bool,
        #[arg(short = 'F', long)]
        literal: bool,
        #[arg(short = 'i')]
        case_insensitive: bool,
        /// Smart case: case-insensitive if pattern is all lowercase
        #[arg(short = 'S', long)]
        smart_case: bool,
        #[arg(short = 'l')]
        files_only: bool,
        #[arg(short = 'c', long)]
        count: bool,
        #[arg(short = 'm', long)]
        max_count: Option<usize>,
        #[arg(short = 'q', long)]
        quiet: bool,
        #[arg(short = 'C', long, default_value = "0")]
        context: usize,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        glob: Option<String>,
        #[arg(long = "type")]
        file_type: Option<String>,
        /// Follow symbolic links
        #[arg(long)]
        follow: bool,
        /// Additional patterns (combined with OR)
        #[arg(short = 'e', long = "regexp")]
        extra_patterns: Vec<String>,
    },
    /// Update the index incrementally
    Update {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show index status
    Status {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Upgrade frg to the latest release from GitHub
    Upgrade,
    /// Initialize frg for this project
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Install git hook to auto-index on commit
        #[arg(long)]
        hook: bool,
    },
    /// Watch for file changes and auto-update the index
    Watch {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Generate man page
    Man,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Search and replace in files
    Replace {
        /// Regex pattern to search for
        pattern: String,
        /// Replacement string (supports $1, $2 for capture groups)
        replacement: String,
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Actually write changes (default is dry-run preview)
        #[arg(long)]
        write: bool,
        /// Case-insensitive
        #[arg(short = 'i')]
        case_insensitive: bool,
        /// Treat pattern as fixed string
        #[arg(short = 'F', long)]
        literal: bool,
        /// Filter files by glob
        #[arg(long)]
        glob: Option<String>,
        /// Filter by file type
        #[arg(long = "type")]
        file_type: Option<String>,
    },
}

pub fn run() -> Result<()> {
    // Merge config file args with CLI args
    let config_args = load_config_args();
    let all_args = std::iter::once(std::env::args().next().unwrap_or_default())
        .chain(config_args)
        .chain(std::env::args().skip(1));
    let cli = Cli::parse_from(all_args);

    let result = match cli.command {
        Commands::Index {
            path,
            max_filesize,
            force: _,
            follow,
        } => {
            let root = std::fs::canonicalize(&path)?;
            eprintln!("Indexing {}...", root.display());
            index::build_index_follow(&root, max_filesize, follow)?;
            let gen = index::current_generation(&root)?;
            let meta = index::meta::IndexMeta::read(&gen.join("meta.json"))?;
            eprintln!(
                "Done. {} files, {} n-grams.",
                meta.file_count, meta.ngram_count
            );
            Ok(())
        }
        Commands::Search {
            pattern,
            path,
            no_index,
            literal,
            case_insensitive,
            smart_case,
            files_only,
            count,
            max_count,
            quiet,
            context,
            json,
            glob,
            file_type,
            follow,
            extra_patterns,
        } => {
            let root = std::fs::canonicalize(&path)?;

            // Combine main pattern with extra patterns using alternation
            let combined_pattern = if extra_patterns.is_empty() {
                pattern.clone()
            } else {
                let mut all = vec![pattern.clone()];
                all.extend(extra_patterns);
                all.iter()
                    .map(|p| format!("(?:{})", p))
                    .collect::<Vec<_>>()
                    .join("|")
            };

            // Smart case: if pattern is all lowercase and -i is not set, enable case-insensitive
            let effective_ci = case_insensitive
                || (smart_case && !combined_pattern.chars().any(|c| c.is_uppercase()));

            let opts = SearchOptions {
                case_insensitive: effective_ci,
                files_only,
                count,
                max_count,
                quiet,
                literal,
                context,
                no_index,
                glob_pattern: glob,
                file_type,
                json,
                follow,
            };

            let use_color = output::color::should_color();

            // Use buffered stdout
            use std::io::Write;
            let stdout = std::io::stdout();
            let mut out = std::io::BufWriter::new(stdout.lock());

            // Check if pattern qualifies for streaming fast path
            let effective_pattern = if opts.literal {
                regex::escape(&combined_pattern)
            } else {
                combined_pattern.clone()
            };
            let effective_pattern = if opts.case_insensitive {
                format!("(?i){}", effective_pattern)
            } else {
                effective_pattern
            };
            let kind = search::classify_pattern(&effective_pattern);

            if kind != search::PatternKind::Normal && context == 0 && !json {
                // Streaming fast path: no collection, no Match structs
                let n = search::search_streaming(
                    &root,
                    &combined_pattern,
                    &opts,
                    &mut out,
                    use_color,
                )?;
                if quiet {
                    process::exit(if n == 0 { 1 } else { 0 });
                }
                if n == 0 {
                    process::exit(1);
                }
            } else {
                // Standard path: collect matches, format output
                let matches = search::search(&root, &combined_pattern, &opts)?;

                if quiet {
                    process::exit(if matches.is_empty() { 1 } else { 0 });
                }
                if matches.is_empty() {
                    process::exit(1);
                }

                if files_only {
                    for f in &output::unique_files(&matches) {
                        let _ = writeln!(out, "{}", f);
                    }
                } else if count {
                    let mut counts = std::collections::HashMap::new();
                    for m in &matches {
                        *counts.entry(m.file_path.as_str()).or_insert(0usize) += 1;
                    }
                    let mut sorted: Vec<_> = counts.into_iter().collect();
                    sorted.sort_by_key(|(p, _)| p.to_string());
                    for (p, c) in sorted {
                        let _ = writeln!(out, "{}", output::format_count(p, c, use_color));
                    }
                } else if json {
                    for m in &matches {
                        let _ = writeln!(out, "{}", output::format_match_json(m));
                    }
                } else {
                    for m in &matches {
                        for (ln, content) in &m.context_before {
                            let _ = writeln!(
                                out,
                                "{}",
                                output::format_context_line(*ln, content, &m.file_path, use_color)
                            );
                        }
                        let _ = writeln!(out, "{}", output::format_match(m, use_color));
                        for (ln, content) in &m.context_after {
                            let _ = writeln!(
                                out,
                                "{}",
                                output::format_context_line(*ln, content, &m.file_path, use_color)
                            );
                        }
                    }
                }
            }
            Ok(())
        }
        Commands::Update { path } => {
            let root = std::fs::canonicalize(&path)?;
            eprintln!("Updating index...");
            index::update_index(&root, 10 * 1024 * 1024)?;
            let gen = index::current_generation(&root)?;
            let meta = index::meta::IndexMeta::read(&gen.join("meta.json"))?;
            if meta.overlay_file_count > 0 || meta.tombstone_count > 0 {
                eprintln!(
                    "Done. {} base files, {} overlay files, {} tombstoned.",
                    meta.file_count, meta.overlay_file_count, meta.tombstone_count
                );
            } else {
                eprintln!("Done. No changes detected.");
            }
            Ok(())
        }
        Commands::Status { path } => {
            let root = std::fs::canonicalize(&path)?;
            index::index_status(&root)
        }
        Commands::Watch { path } => {
            use notify::{RecursiveMode, Watcher};
            use std::sync::mpsc;
            use std::time::Duration;

            let root = std::fs::canonicalize(&path)?;

            // Ensure index exists
            if index::current_generation(&root).is_err() {
                eprintln!("No index found. Building...");
                index::build_index(&root, 10 * 1024 * 1024)?;
                let gen = index::current_generation(&root)?;
                let meta = index::meta::IndexMeta::read(&gen.join("meta.json"))?;
                eprintln!("Done. {} files indexed.", meta.file_count);
            }

            eprintln!(
                "Watching {} for changes... (Ctrl+C to stop)",
                root.display()
            );

            let (tx, rx) = mpsc::channel();

            let mut watcher = notify::recommended_watcher(
                move |res: Result<notify::Event, notify::Error>| {
                    if let Ok(event) = res {
                        use notify::EventKind;
                        match event.kind {
                            EventKind::Create(_)
                            | EventKind::Modify(_)
                            | EventKind::Remove(_) => {
                                // Skip .frg directory changes
                                let all_frg = event
                                    .paths
                                    .iter()
                                    .all(|p| p.components().any(|c| c.as_os_str() == ".frg"));
                                if !all_frg {
                                    let _ = tx.send(());
                                }
                            }
                            _ => {}
                        }
                    }
                },
            )
            .map_err(|e| anyhow::anyhow!("failed to create file watcher: {}", e))?;

            watcher
                .watch(&root, RecursiveMode::Recursive)
                .map_err(|e| anyhow::anyhow!("failed to watch directory: {}", e))?;

            while rx.recv().is_ok() {
                // Debounce: drain any queued events for 500ms
                let deadline =
                    std::time::Instant::now() + Duration::from_millis(500);
                while let Ok(()) = rx.recv_timeout(
                    deadline.saturating_duration_since(std::time::Instant::now()),
                ) {
                    // drain
                }

                eprint!("Change detected, updating... ");
                match index::update_index(&root, 10 * 1024 * 1024) {
                    Ok(()) => {
                        if let Ok(gen) = index::current_generation(&root) {
                            if let Ok(meta) = index::meta::IndexMeta::read(
                                &gen.join("meta.json"),
                            ) {
                                if meta.overlay_file_count > 0
                                    || meta.tombstone_count > 0
                                {
                                    eprintln!(
                                        "{} overlay, {} tombstoned.",
                                        meta.overlay_file_count,
                                        meta.tombstone_count
                                    );
                                } else {
                                    eprintln!("no changes.");
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("error: {}", e),
                }
            }
            Ok(())
        }
        Commands::Upgrade => self_upgrade(),
        Commands::Init { path, hook } => {
            let root = std::fs::canonicalize(&path)?;

            // Create .frgignore if it doesn't exist
            let ignore_path = root.join(".frgignore");
            if !ignore_path.exists() {
                let mut ignore_content = String::from("# frg ignore patterns\n");

                // Auto-detect project type and add relevant ignores
                if root.join("package.json").exists() {
                    ignore_content.push_str("node_modules/\ndist/\n.next/\ncoverage/\n");
                    eprintln!("Detected Node.js project");
                }
                if root.join("Cargo.toml").exists() {
                    ignore_content.push_str("target/\n");
                    eprintln!("Detected Rust project");
                }
                if root.join("go.mod").exists() {
                    ignore_content.push_str("vendor/\n");
                    eprintln!("Detected Go project");
                }
                if root.join("requirements.txt").exists() || root.join("pyproject.toml").exists() {
                    ignore_content.push_str("__pycache__/\n*.pyc\n.venv/\nvenv/\n");
                    eprintln!("Detected Python project");
                }
                if root.join("Gemfile").exists() {
                    ignore_content.push_str("vendor/bundle/\n");
                    eprintln!("Detected Ruby project");
                }

                std::fs::write(&ignore_path, &ignore_content)?;
                eprintln!("Created {}", ignore_path.display());
            } else {
                eprintln!(".frgignore already exists, skipping");
            }

            // Install git hook if requested
            if hook {
                let hooks_dir = root.join(".git/hooks");
                if hooks_dir.exists() {
                    let hook_path = hooks_dir.join("post-commit");
                    let hook_content = "#!/bin/sh\nfrg update . &\n";
                    std::fs::write(&hook_path, hook_content)?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(
                            &hook_path,
                            std::fs::Permissions::from_mode(0o755),
                        )?;
                    }
                    eprintln!("Installed post-commit hook for auto-indexing");
                } else {
                    eprintln!("Not a git repository, skipping hook installation");
                }
            }

            // Build index
            eprintln!("Building index...");
            index::build_index(&root, 10 * 1024 * 1024)?;
            let gen = index::current_generation(&root)?;
            let meta = index::meta::IndexMeta::read(&gen.join("meta.json"))?;
            eprintln!(
                "Done. {} files, {} n-grams.",
                meta.file_count, meta.ngram_count
            );
            Ok(())
        }
        Commands::Man => {
            let cmd = Cli::command();
            let man = clap_mangen::Man::new(cmd);
            let mut buf = Vec::new();
            man.render(&mut buf)?;
            std::io::stdout().write_all(&buf)?;
            Ok(())
        }
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "frg", &mut std::io::stdout());
            Ok(())
        }
        Commands::Replace {
            pattern,
            replacement,
            path,
            write,
            case_insensitive,
            literal,
            glob,
            file_type,
        } => {
            let root = std::fs::canonicalize(&path)?;

            let effective_pattern = if literal {
                regex::escape(&pattern)
            } else {
                pattern.clone()
            };
            let effective_pattern = if case_insensitive {
                format!("(?i){}", effective_pattern)
            } else {
                effective_pattern
            };
            let replacement_bytes = if literal {
                replacement.as_bytes().to_vec()
            } else {
                replacement.into_bytes()
            };

            let re = regex::bytes::Regex::new(&effective_pattern)?;

            // Find files with matches using existing search
            let opts = SearchOptions {
                case_insensitive,
                literal,
                glob_pattern: glob,
                file_type,
                ..Default::default()
            };
            let matches = search::search(&root, &pattern, &opts)?;

            // Get unique file paths from matches
            let mut file_paths: Vec<String> = matches.iter().map(|m| m.file_path.clone()).collect();
            file_paths.sort();
            file_paths.dedup();

            let stdout = std::io::stdout();
            let mut out = std::io::BufWriter::new(stdout.lock());
            let use_color = output::color::should_color();

            let mut total_replacements = 0;
            let mut total_files = 0;

            if write {
                // Write mode
                for file_path in &file_paths {
                    let p = std::path::Path::new(file_path);
                    match search::replace::write_replacement(p, &re, &replacement_bytes) {
                        Ok(0) => {}
                        Ok(n) => {
                            total_replacements += n;
                            total_files += 1;
                            let _ = writeln!(out, "{}: {} replacements", file_path, n);
                        }
                        Err(e) => eprintln!("error: {}: {}", file_path, e),
                    }
                }
                eprintln!(
                    "Wrote {} replacements in {} files.",
                    total_replacements, total_files
                );
            } else {
                // Preview mode (dry-run)
                for file_path in &file_paths {
                    let p = std::path::Path::new(file_path);
                    if let Some(result) =
                        search::replace::replace_in_file(p, &re, &replacement_bytes)
                    {
                        total_replacements += result.replacements;
                        total_files += 1;
                        if use_color {
                            let _ = writeln!(out, "\x1b[36m{}\x1b[0m:", result.file_path);
                        } else {
                            let _ = writeln!(out, "{}:", result.file_path);
                        }
                        for ((ln, orig), (_, repl)) in
                            result.original_lines.iter().zip(result.replaced_lines.iter())
                        {
                            if use_color {
                                let _ = writeln!(
                                    out,
                                    "  \x1b[32m{:>4}\x1b[0m | \x1b[31m- {}\x1b[0m",
                                    ln, orig
                                );
                                let _ = writeln!(out, "       | \x1b[32m+ {}\x1b[0m", repl);
                            } else {
                                let _ = writeln!(out, "  {:>4} | - {}", ln, orig);
                                let _ = writeln!(out, "       | + {}", repl);
                            }
                        }
                    }
                }
                if total_files > 0 {
                    eprintln!(
                        "Preview: {} lines changed in {} files. Use --write to apply.",
                        total_replacements, total_files
                    );
                } else {
                    eprintln!("No matches found.");
                    process::exit(1);
                }
            }
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("frg: {}", e);
        process::exit(2);
    }
    Ok(())
}

fn self_upgrade() -> Result<()> {
    const REPO: &str = "qhkm/fastripgrep";
    let current_version = env!("CARGO_PKG_VERSION");

    eprintln!("Current version: v{}", current_version);
    eprintln!("Checking for updates...");

    // Get latest release tag from GitHub API
    let output = std::process::Command::new("curl")
        .args(["-fsSL", &format!("https://api.github.com/repos/{}/releases/latest", REPO)])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("failed to check for updates (no internet or no releases yet)");
    }

    let body = String::from_utf8_lossy(&output.stdout);

    // Extract tag_name from JSON (minimal parsing, no serde dependency in hot path)
    let tag = body
        .lines()
        .find(|l| l.contains("\"tag_name\""))
        .and_then(|l| {
            let after_key = &l[l.find("\"tag_name\"")? + 10..];
            let q1 = after_key.find('"')? + 1;
            let q2 = after_key[q1..].find('"')? + q1;
            Some(after_key[q1..q2].to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("could not parse latest release tag"))?;

    let latest_version = tag.trim_start_matches('v');
    if latest_version == current_version {
        eprintln!("Already up to date (v{}).", current_version);
        return Ok(());
    }

    eprintln!("Upgrading v{} -> {}...", current_version, tag);

    // Detect platform
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let target = match (os, arch) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        _ => anyhow::bail!("unsupported platform: {}-{}", os, arch),
    };

    let url = format!(
        "https://github.com/{}/releases/download/{}/frg-{}-{}.tar.gz",
        REPO, tag, tag, target
    );

    // Download to temp file
    let tmp_dir = std::env::temp_dir().join("frg-upgrade");
    std::fs::create_dir_all(&tmp_dir)?;
    let tarball = tmp_dir.join("frg.tar.gz");

    let status = std::process::Command::new("curl")
        .args(["-fsSL", &url, "-o"])
        .arg(&tarball)
        .status()?;

    if !status.success() {
        anyhow::bail!("failed to download {}", url);
    }

    // Extract
    let status = std::process::Command::new("tar")
        .args(["xzf"])
        .arg(&tarball)
        .arg("-C")
        .arg(&tmp_dir)
        .status()?;

    if !status.success() {
        anyhow::bail!("failed to extract archive");
    }

    // Replace current binary
    let new_binary = tmp_dir.join("frg");
    let current_binary = std::env::current_exe()?;

    // On Unix, we can replace a running binary by renaming
    let backup = current_binary.with_extension("old");
    if backup.exists() {
        std::fs::remove_file(&backup)?;
    }
    std::fs::rename(&current_binary, &backup)?;

    match std::fs::copy(&new_binary, &current_binary) {
        Ok(_) => {
            // Set executable permission
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&current_binary, std::fs::Permissions::from_mode(0o755))?;
            }
            let _ = std::fs::remove_file(&backup);
            let _ = std::fs::remove_dir_all(&tmp_dir);
            eprintln!("Upgraded to {} successfully.", tag);
        }
        Err(e) => {
            // Rollback
            let _ = std::fs::rename(&backup, &current_binary);
            let _ = std::fs::remove_dir_all(&tmp_dir);
            anyhow::bail!("failed to install new binary: {}. Rolled back.", e);
        }
    }

    Ok(())
}
