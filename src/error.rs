//! Error types for `SnapFind`

use arrayvec::ArrayString;
use thiserror::Error;

/// Maximum length of error messages
pub const MAX_ERROR_LENGTH: usize = 256;

/// Custom result type for `SnapFind` operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for `SnapFind`
///
/// # Design
/// - All errors are stack-allocated
/// - String buffers are stack-allocated with a fixed `MAX_ERROR_LENGTH`
/// - No heap allocation during error handling
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

    /// Search engine error with fixed-size message buffer
    #[error("Error: {0}")]
    Search(Box<ArrayString<MAX_ERROR_LENGTH>>),
}

impl Error {
    /// Create a new search error
    ///
    /// # Design
    /// - Message buffer is fixed-size (`MAX_ERROR_LENGTH`)
    /// - No heap allocation
    pub fn search(msg: &str) -> Self {
        let mut buf = ArrayString::new();
        // Try to write the message, truncate if too long
        let _ = buf.try_push_str(msg);
        Self::Search(Box::new(buf))
    }

    /// Get a user-friendly error message with action items
    #[must_use]
    pub fn user_message(&self) -> ArrayString<MAX_ERROR_LENGTH> {
        let mut msg = ArrayString::new();
        match self {
            Self::Io(e) => {
                let _ = msg.try_push_str(&format!(
                    "Error: {e}\nTip: Check file permissions and try again"
                ));
            },
            Self::DepthExceeded => {
                let _ = msg.try_push_str(
                    "Error: Directory structure too deep (max 1000 levels)\nTip: Try indexing a \
                     shallower directory",
                );
            },
            Self::FileCountExceeded => {
                let _ = msg.try_push_str(
                    "Error: Too many files (max 1,000,000)\nTip: Try indexing a smaller directory",
                );
            },
            Self::FileSizeExceeded => {
                let _ = msg.try_push_str(
                    "Error: File too large (max 10MB)\nTip: Large files are skipped during \
                     indexing",
                );
            },
            Self::PathTooLong => {
                let _ = msg.try_push_str(
                    "Error: Path too long (max 255 characters)\nTip: Try moving files to a \
                     shorter path",
                );
            },
            Self::Search(search_msg) => {
                if search_msg.contains("No index found") {
                    let _ = msg.try_push_str(search_msg);
                } else {
                    let _ = msg.try_push_str("Error: ");
                    let _ = msg.try_push_str(search_msg);
                    let _ = msg.try_push_str("\nTip: Try simplifying your search");
                }
            },
        }
        msg
    }
}
