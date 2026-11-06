use std::fmt;

/// An error tied to the source line that caused it, so messages can tell the
/// user *where* things went wrong.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    /// The file that caused the error, when known.
    pub file: Option<String>,
    pub line: usize,
    pub message: String,
}

impl Error {
    pub fn new(line: usize, message: impl Into<String>) -> Self {
        Error {
            file: None,
            line,
            message: message.into(),
        }
    }

    /// Attaches a file name without overwriting an inner error's own file.
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        if self.file.is_none() {
            self.file = Some(file.into());
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{file}:line {}: {}", self.line, self.message),
            None => write!(f, "line {}: {}", self.line, self.message),
        }
    }
}

impl std::error::Error for Error {}
