#![deny(
    warnings,
    missing_debug_implementations,
    missing_docs,
    clippy::all,
    clippy::pedantic,
    clippy::nursery
)]
//! `SnapFind` - Fast file search tool that understands content.

mod alloc;
mod crawler;
mod error;
mod search;
mod text;
mod types;

use alloc::TrackingAllocator;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use clap_cargo::style::CLAP_STYLING;
use error::{Error, Result};
use text::TextDetector;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

/// CLI arguments for `SnapFind`
#[derive(Parser, Debug)]
#[command(author, version, about, styles = CLAP_STYLING)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Available commands
#[derive(Subcommand, Debug)]
enum Command {
    /// Index a directory for searching
    Index {
        /// Directory to index
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Search for files
    Search {
        /// Search query
        query: String,
        /// Directory to search in (must be indexed first)
        #[arg(default_value = ".")]
        dir:   PathBuf,
    },
}

/// Get the index file path for a directory
fn get_index_path(dir: &Path) -> PathBuf {
    dir.join(".snapfind_index")
}

/// Index a directory for searching
fn index_directory(dir: &Path) -> Result<()> {
    println!("Indexing directory: {}", dir.display());

    let mut engine = search::SearchEngine::new();
    let mut crawler = crawler::Crawler::new(dir)?;
    let mut detector = TextDetector::new();
    let mut total_files = 0;
    let mut last_progress = 0;
    let mut had_errors = false;

    // Track progress invariants
    let mut last_processed = 0;
    let mut last_dirs = 0;

    while let Some(files) = crawler.process_next()? {
        let (processed, max_files, dirs) = crawler.progress();

        // Assert progress invariants
        assert!(processed >= last_processed, "File count must not decrease");
        assert!(dirs >= last_dirs, "Directory count must not decrease");
        last_processed = processed;
        last_dirs = dirs;

        for file in files {
            // Read initial sample for text detection
            match fs::read(&file) {
                Ok(content) => {
                    // Validate text content
                    let validation = detector.validate(&content);
                    if validation.is_valid_text() {
                        // Log file type information
                        if processed >= last_progress + 100 {
                            println!(
                                "Progress: {processed}/{max_files} files indexed ({dirs} \
                                 directories found)"
                            );
                            println!(
                                "Last file: {} ({:?}, confidence: {}%)",
                                file.display(),
                                validation.mime_type(),
                                validation.confidence()
                            );
                            last_progress = processed;
                        }

                        match engine
                            .add_document(&file, std::str::from_utf8(&content).unwrap_or(""))
                        {
                            Ok(()) => {
                                total_files += 1;
                            },
                            Err(e) => {
                                // Stop on first document error
                                eprintln!("\nIndexing stopped due to error.");
                                return Err(e);
                            },
                        }
                    }
                },
                Err(e) => {
                    had_errors = true;
                    eprintln!("Error: Failed to read {}: {e}", file.display());
                    // Continue with next file
                },
            }
        }
    }

    // Final status
    if total_files == 0 {
        if had_errors {
            return Err(Error::Search(
                "Failed to index any files due to errors. Check file permissions and try again."
                    .into(),
            ));
        }
        println!("No files were indexed. Make sure the directory contains text files.");
        return Ok(());
    }

    println!("\nIndexing completed:");
    println!("- Files indexed: {total_files}");
    let (_, _, dirs) = crawler.progress();
    println!("- Directories processed: {dirs}");

    // Save the index
    let index_path = get_index_path(dir);
    engine.save(&index_path)?;
    println!("- Index saved to {}", index_path.display());

    // End initialization phase
    ALLOCATOR.end_init();
    println!("- Peak memory usage: {} bytes", ALLOCATOR.peak());

    Ok(())
}

/// Search for files matching a query
fn search_files(query: &str, dir: &Path) -> Result<()> {
    println!("Searching for: {query} in {}", dir.display());

    // Validate directory
    if !dir.exists() {
        return Err(Error::Search(format!("Directory not found: {}", dir.display())));
    }
    if !dir.is_dir() {
        return Err(Error::Search(format!("Not a directory: {}", dir.display())));
    }

    // Load the index
    let index_path = get_index_path(dir);
    if !index_path.exists() {
        return Err(Error::Search(format!(
            "No index found for {}. Run 'snapfind index' first.",
            dir.display()
        )));
    }

    let engine = search::SearchEngine::load(&index_path)?;
    let results = engine.search(query)?;

    if results.is_empty() {
        println!("\nNo matches found for query: {query}");
        println!("Tips:");
        println!("  - Try using fewer or simpler search terms");
        println!("  - Check if the directory has been indexed recently");
        println!(
            "  - Make sure the directory contains text files (we support plain text, markdown, \
             source code, and config files)"
        );
        return Ok(());
    }

    println!("\nFound {} matches:", results.len());
    println!("Score | Type | Path");
    println!("------|------|------");

    for result in results {
        // Determine match type based on score
        let match_type = if result.score > 60.0 {
            "name+content"
        } else if result.score > 40.0 {
            "name"
        } else {
            "content"
        };

        println!("{:>5.1}% | {:<4} | {}", result.score, match_type, result.path.display());
    }

    Ok(())
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Index { dir } => {
            if !dir.exists() {
                Err(Error::Search(format!("Directory not found: {}", dir.display())))
            } else if !dir.is_dir() {
                Err(Error::Search(format!("Not a directory: {}", dir.display())))
            } else {
                index_directory(&dir)
            }
        },
        Command::Search { query, dir } => {
            if !dir.exists() {
                Err(Error::Search(format!("Directory not found: {}", dir.display())))
            } else if !dir.is_dir() {
                Err(Error::Search(format!("Not a directory: {}", dir.display())))
            } else if query.is_empty() {
                Err(Error::Search("Search query cannot be empty".into()))
            } else {
                search_files(&query, &dir)
            }
        },
    };

    if let Err(e) = result {
        eprintln!("{}", e.user_message());
        std::process::exit(1);
    }
}
