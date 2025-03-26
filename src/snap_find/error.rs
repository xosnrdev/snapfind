use arrayvec::ArrayString;
use thiserror::Error;

pub const MAX_ERROR_LENGTH: usize = 256;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Error: Maximum directory depth of 1000 exceeded")]
    DepthExceeded,

    #[error("Error: Maximum file count of 1,000,000 exceeded")]
    FileCountExceeded,

    #[error("Error: Maximum file size of 10MB exceeded")]
    FileSizeExceeded,

    #[error("Error: Path length exceeded 255 characters")]
    PathTooLong,

    #[error("Error: {0}")]
    Search(Box<ArrayString<MAX_ERROR_LENGTH>>),
}

impl Error {
    #[must_use]
    pub fn search(msg: &str) -> Self {
        let mut buf = ArrayString::new();
        let _ = buf.try_push_str(msg);
        Self::Search(Box::new(buf))
    }

    #[must_use]
    pub fn user_message(&self) -> ArrayString<MAX_ERROR_LENGTH> {
        let mut msg = ArrayString::new();
        match self {
            Self::Io(e) => {
                let _ = msg.try_push_str(&format!(
                    "Error: {e}\nTip: Check file permissions and try again"
                ));
            }
            Self::DepthExceeded => {
                let _ = msg.try_push_str(
                    "Error: Directory structure too deep (max 1000 levels)\nTip: Try indexing a \
                     shallower directory",
                );
            }
            Self::FileCountExceeded => {
                let _ = msg.try_push_str(
                    "Error: Too many files (max 1,000,000)\nTip: Try indexing a smaller directory",
                );
            }
            Self::FileSizeExceeded => {
                let _ = msg.try_push_str(
                    "Error: File too large (max 10MB)\nTip: Large files are skipped during \
                     indexing",
                );
            }
            Self::PathTooLong => {
                let _ = msg.try_push_str(
                    "Error: Path too long (max 255 characters)\nTip: Try moving files to a \
                     shorter path",
                );
            }
            Self::Search(search_msg) => {
                if search_msg.contains("No index found") {
                    let _ = msg.try_push_str(search_msg);
                } else {
                    let _ = msg.try_push_str("Error: ");
                    let _ = msg.try_push_str(search_msg);
                    let _ = msg.try_push_str("\nTip: Try simplifying your search");
                }
            }
        }
        msg
    }
}
