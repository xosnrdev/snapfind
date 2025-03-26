#[derive(Debug)]
pub struct SnapError {
    error: Option<anyhow::Error>,
    code: i32,
}

impl SnapError {
    pub fn silent(code: i32) -> Self {
        Self { error: None, code }
    }

    pub fn message<T: Into<anyhow::Error>>(e: T) -> Self {
        Self {
            error: Some(e.into()),
            code: 101,
        }
    }

    pub fn with_code(msg: impl ToString, code: i32) -> Self {
        Self {
            error: Some(anyhow::anyhow!("{}", msg.to_string())),
            code,
        }
    }

    pub fn code(&self) -> i32 {
        self.code
    }
}

macro_rules! process_error_from {
    ($from:ty) => {
        impl From<$from> for SnapError {
            fn from(error: $from) -> Self {
                Self::message(error)
            }
        }
    };
}

process_error_from!(anyhow::Error);
process_error_from!(std::io::Error);

impl From<String> for SnapError {
    fn from(msg: String) -> Self {
        Self::message(anyhow::anyhow!("{}", msg))
    }
}

impl From<&str> for SnapError {
    fn from(msg: &str) -> Self {
        Self::message(anyhow::anyhow!("{}", msg))
    }
}

impl From<i32> for SnapError {
    fn from(code: i32) -> Self {
        Self::silent(code)
    }
}

impl std::fmt::Display for SnapError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(ref error) = self.error {
            write!(f, "{}", error)
        } else {
            write!(f, "Error code: {}", self.code)
        }
    }
}

impl std::error::Error for SnapError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.as_ref().map(|e| e.as_ref())
    }
}

pub type SnapResult<T> = anyhow::Result<T>;
