use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

use crate::index;
use crate::search::{self, SearchOptions};
use crate::output;

#[derive(Parser)]
#[command(name = "rsgrep", version, about = "Fast regex search with sparse n-gram indexing")]
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
    },
    /// Rebuild the index (full rebuild; incremental updates planned for v0.2)
    Update {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show index status
    Status {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Index { path, max_filesize, force: _ } => {
            let root = std::fs::canonicalize(&path)?;
            eprintln!("Indexing {}...", root.display());
            index::build_index(&root, max_filesize)?;
            let gen = index::current_generation(&root)?;
            let meta = index::meta::IndexMeta::read(&gen.join("meta.json"))?;
            eprintln!("Done. {} files, {} n-grams.", meta.file_count, meta.ngram_count);
            Ok(())
        }
        Commands::Search {
            pattern,
            path,
            no_index,
            literal,
            case_insensitive,
            files_only,
            count,
            max_count,
            quiet,
            context,
            json,
            glob,
            file_type,
        } => {
            let root = std::fs::canonicalize(&path)?;
            let opts = SearchOptions {
                case_insensitive,
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
            };

            let matches = search::search(&root, &pattern, &opts)?;

            if quiet {
                process::exit(if matches.is_empty() { 1 } else { 0 });
            }
            if matches.is_empty() {
                process::exit(1);
            }

            let use_color = output::color::should_color();

            // Use buffered stdout to avoid per-line flush overhead
            use std::io::Write;
            let stdout = std::io::stdout();
            let mut out = std::io::BufWriter::new(stdout.lock());

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
                        let _ = writeln!(out, "{}", output::format_context_line(*ln, content, &m.file_path, use_color));
                    }
                    let _ = writeln!(out, "{}", output::format_match(m, use_color));
                    for (ln, content) in &m.context_after {
                        let _ = writeln!(out, "{}", output::format_context_line(*ln, content, &m.file_path, use_color));
                    }
                }
            }
            Ok(())
        }
        Commands::Update { path } => {
            let root = std::fs::canonicalize(&path)?;
            eprintln!("Rebuilding index...");
            index::build_index(&root, 10 * 1024 * 1024)?;
            eprintln!("Done (full rebuild; incremental updates planned for v0.2).");
            Ok(())
        }
        Commands::Status { path } => {
            let root = std::fs::canonicalize(&path)?;
            index::index_status(&root)
        }
    };

    if let Err(e) = result {
        eprintln!("rsgrep: {}", e);
        process::exit(2);
    }
    Ok(())
}
