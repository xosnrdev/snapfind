mod snap_find;

use std::path::{Path, PathBuf};
use std::{fs, process};

use clap::{Parser, Subcommand};
use clap_cargo::style::CLAP_STYLING;
use snap_find::error::{SnapError, SnapResult};
use snap_find::text::TextDetector;
use snap_find::{crawler, search};

#[derive(Debug, Parser)]
#[command(author, version, about, display_name="", styles = CLAP_STYLING)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
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
        dir: PathBuf,
    },
}

fn get_index_path(dir: &Path) -> PathBuf {
    dir.join(".snapfind_index")
}

fn index_directory(dir: &Path) -> SnapResult<()> {
    println!("Indexing directory: {}", dir.display());

    let mut engine = search::SearchEngine::new();
    let mut crawler = crawler::Crawler::new(dir)?;
    let mut detector = TextDetector::new();
    let mut total_files = 0;
    let mut last_progress = 0;
    let mut had_errors = false;

    let mut last_processed = 0;
    let mut last_dirs = 0;

    while let Some(files) = crawler.process_next()? {
        let (processed, max_files, dirs) = crawler.progress();

        assert!(processed >= last_processed, "File count must not decrease");
        assert!(dirs >= last_dirs, "Directory count must not decrease");
        last_processed = processed;
        last_dirs = dirs;

        for file in files {
            match fs::read(&file) {
                Ok(content) => {
                    let validation = detector.validate(&content);
                    if validation.is_valid_text() {
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
                            }
                            Err(e) => {
                                eprintln!("\nIndexing stopped due to error.");
                                return Err(e);
                            }
                        }
                    }
                }
                Err(e) => {
                    had_errors = true;
                    eprintln!("Error: Failed to read {}: {e}", file.display());
                }
            }
        }
    }

    if total_files == 0 {
        if had_errors {
            return Err(anyhow::Error::from(SnapError::with_code(
                "Failed to index any files due to errors. Check file permissions and try again.",
                search::ERROR_INVALID_INDEX,
            )));
        }
        println!("No files were indexed. Make sure the directory contains text files.");
        return Ok(());
    }

    println!("\nIndexing completed:");
    println!("- Files indexed: {total_files}");
    let (_, _, dirs) = crawler.progress();
    println!("- Directories processed: {dirs}");

    let index_path = get_index_path(dir);
    engine.save(&index_path)?;
    println!("- Index saved to {}", index_path.display());

    Ok(())
}

fn search_files(query: &str, dir: &Path) -> SnapResult<()> {
    println!("Searching for: {query} in {}", dir.display());

    search::validate_query(query)?;

    if !dir.exists() {
        return Err(anyhow::Error::from(SnapError::with_code(
            format!("Directory not found: {}", dir.display()),
            search::ERROR_INVALID_INDEX,
        )));
    }
    if !dir.is_dir() {
        return Err(anyhow::Error::from(SnapError::with_code(
            format!("Not a directory: {}", dir.display()),
            search::ERROR_INVALID_INDEX,
        )));
    }

    let engine = if let Ok(loaded) = search::SearchEngine::load(&get_index_path(dir)) {
        loaded
    } else {
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
                Err(anyhow::Error::from(SnapError::with_code(
                    format!("Directory not found: {}", dir.display()),
                    search::ERROR_INVALID_INDEX,
                )))
            } else if !dir.is_dir() {
                Err(anyhow::Error::from(SnapError::with_code(
                    format!("Not a directory: {}", dir.display()),
                    search::ERROR_INVALID_INDEX,
                )))
            } else {
                index_directory(&dir)
            }
        }
        Command::Search { query, dir } => {
            if !dir.exists() {
                Err(anyhow::Error::from(SnapError::with_code(
                    format!("Directory not found: {}", dir.display()),
                    search::ERROR_INVALID_INDEX,
                )))
            } else if !dir.is_dir() {
                Err(anyhow::Error::from(SnapError::with_code(
                    format!("Not a directory: {}", dir.display()),
                    search::ERROR_INVALID_INDEX,
                )))
            } else if query.is_empty() {
                Err(anyhow::Error::from(SnapError::with_code(
                    "Search query cannot be empty",
                    search::ERROR_INVALID_QUERY,
                )))
            } else {
                search_files(&query, &dir)
            }
        }
    };

    if let Err(e) = result {
        eprintln!("{}", e);
        if let Some(err) = e.downcast_ref::<snap_find::error::SnapError>() {
            process::exit(err.code());
        } else {
            process::exit(1);
        }
    }
}
