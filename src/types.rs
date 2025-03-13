//! Common types and constants for `SnapFind`

use std::path::PathBuf;

/// Maximum directory depth allowed
pub const MAX_DEPTH: usize = 1_000;

/// Maximum number of files to process
pub const MAX_FILES: usize = 1_000;

/// Maximum file size in bytes (10MB)
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum path length in characters
pub const MAX_PATH_LENGTH: usize = 255;

/// A search result with its relevance score
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Path to the matching file
    pub path:  PathBuf,
    /// Relevance score (0.0 to 1.0)
    pub score: f32,
}

const _: () = {
    assert!(MAX_DEPTH > 0);
    assert!(MAX_FILES > 0);
    assert!(MAX_FILE_SIZE > 0);
    assert!(MAX_PATH_LENGTH > 0);
};
