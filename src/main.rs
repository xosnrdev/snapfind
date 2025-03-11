#![cfg_attr(not(feature = "std"), no_std)]
#![deny(
    warnings,
    missing_debug_implementations,
    missing_docs,
    clippy::all,
    clippy::pedantic,
    clippy::nursery
)]
//! `SnapFind` - Fast file search tool that understands content.

#[cfg(not(feature = "std"))]
extern crate core as std;
#[cfg(feature = "std")]
extern crate std;

mod alloc;
mod crawler;
mod error;
mod search;
mod text;
mod types;

use alloc::TrackingAllocator;
#[cfg(feature = "std")]
use std::fs;
#[cfg(feature = "std")]
use std::path::{Path, PathBuf};

#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};
#[cfg(feature = "cli")]
use clap_cargo::style::CLAP_STYLING;
use error::{Error, Result};
use text::TextDetector;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator::new();

/// CLI arguments for `SnapFind`
#[cfg_attr(feature = "cli", derive(Parser))]
#[derive(Debug)]
#[cfg_attr(feature = "cli", command(author, version, about))]
#[cfg_attr(feature = "cli", command(display_name="", styles = CLAP_STYLING))]
struct Cli {
    #[cfg_attr(feature = "cli", command(subcommand))]
    command: Command,
}

/// Available commands
#[cfg_attr(feature = "cli", derive(Subcommand))]
#[derive(Debug)]
enum Command {
    /// Index a directory for searching
    Index {
        /// Directory to index
        #[cfg_attr(feature = "cli", arg(default_value = "."))]
        dir: PathBuf,
    },
    /// Search for files
    Search {
        /// Search query
        query: String,
        /// Directory to search in (must be indexed first)
        #[cfg_attr(feature = "cli", arg(default_value = "."))]
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
            return Err(Error::search(
                "Failed to index any files due to errors. Check file permissions and try again.",
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

    // Validate query
    search::validate_query(query)?;

    // Validate directory
    if !dir.exists() {
        return Err(Error::search(&format!("Directory not found: {}", dir.display())));
    }
    if !dir.is_dir() {
        return Err(Error::search(&format!("Not a directory: {}", dir.display())));
    }

    // Load or create search engine
    let engine = if let Ok(loaded) = search::SearchEngine::load(&get_index_path(dir)) {
        loaded
    } else {
        // If no index exists, create a new one and scan directory
        let mut new_engine = search::SearchEngine::new();
        let mut crawler = crawler::Crawler::new(dir)?;

        while let Some(files) = crawler.process_next()? {
            for file in files {
                if let Ok(content) = fs::read_to_string(&file) {
                    new_engine.add_document(&file, &content)?;
                }
            }
        }
        new_engine
    };

    // Search using the engine
    let results = engine.search(query)?;

    if results.is_empty() {
        println!("\nNo matches found for query: {query}");
        println!("Tips:");
        println!("  - Try using simpler search terms");
        println!("  - Check if the files exist in the directory");
        println!("  - Make sure you have read permissions for the files");
        return Ok(());
    }

    println!("\nFound {} matches:", results.len());
    println!("Score | Path");
    println!("------|------");

    for result in results {
        println!("{:>5.1}% | {}", result.score, result.path.display());
    }

    Ok(())
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Index { dir } => {
            if !dir.exists() {
                Err(Error::search(&format!("Directory not found: {}", dir.display())))
            } else if !dir.is_dir() {
                Err(Error::search(&format!("Not a directory: {}", dir.display())))
            } else {
                index_directory(&dir)
            }
        },
        Command::Search { query, dir } => {
            if !dir.exists() {
                Err(Error::search(&format!("Directory not found: {}", dir.display())))
            } else if !dir.is_dir() {
                Err(Error::search(&format!("Not a directory: {}", dir.display())))
            } else if query.is_empty() {
                Err(Error::search("Search query cannot be empty"))
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
