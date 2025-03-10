//! Error types for `SnapFind`

use thiserror::Error;

/// Custom result type for `SnapFind` operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types that can occur during `SnapFind` operations
#[derive(Debug, Error)]
pub enum Error {
    /// IO operation failed
    #[error("Error: {0}")]
    Io(#[from] std::io::Error),

    /// Directory depth exceeded the maximum limit
    #[error("Error: Maximum directory depth of 1000 exceeded")]
    DepthExceeded,

    /// File count exceeded the maximum limit
    #[error("Error: Maximum file count of 1,000,000 exceeded")]
    FileCountExceeded,

    /// File size exceeded the maximum limit
    #[error("Error: Maximum file size of 10MB exceeded")]
    FileSizeExceeded,

    /// Path length exceeded the maximum limit
    #[error("Error: Path length exceeded 255 characters")]
    PathTooLong,

    /// Search engine error
    #[error("Error: {0}")]
    Search(String),
}

impl Error {
    /// Get a user-friendly error message with action items
    #[must_use]
    pub fn user_message(&self) -> String {
        match self {
            Self::Io(e) => format!("Error: {e}\nTip: Check file permissions and try again"),
            Self::DepthExceeded => String::from(
                "Error: Directory structure too deep (max 1000 levels)\nTip: Try indexing a \
                 shallower directory",
            ),
            Self::FileCountExceeded => String::from(
                "Error: Too many files (max 1,000,000)\nTip: Try indexing a smaller directory",
            ),
            Self::FileSizeExceeded => String::from(
                "Error: File too large (max 10MB)\nTip: Large files are skipped during indexing",
            ),
            Self::PathTooLong => String::from(
                "Error: Path too long (max 255 characters)\nTip: Try moving files to a shorter \
                 path",
            ),
            Self::Search(msg) => {
                if msg.contains("No index found") {
                    format!("Error: {msg}")
                } else {
                    format!("Error: {msg}\nTip: Try simplifying your search")
                }
            },
        }
    }
}
