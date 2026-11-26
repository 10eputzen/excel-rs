use std::fmt;

/// Stable error codes, mirrored by the JavaScript `ExcelError.code`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    UnsupportedFormat,
    CorruptFile,
    SheetNotFound,
    TooLarge,
    InvalidInput,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::UnsupportedFormat => "UNSUPPORTED_FORMAT",
            ErrorCode::CorruptFile => "CORRUPT_FILE",
            ErrorCode::SheetNotFound => "SHEET_NOT_FOUND",
            ErrorCode::TooLarge => "TOO_LARGE",
            ErrorCode::InvalidInput => "INVALID_INPUT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Error {
            code,
            message: message.into(),
        }
    }

    pub fn corrupt(err: impl fmt::Display) -> Self {
        Error::new(ErrorCode::CorruptFile, err.to_string())
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Error::new(ErrorCode::InvalidInput, message)
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Error::new(ErrorCode::TooLarge, message)
    }

    /// JSON form used across the WASM boundary: `{"code":"…","message":"…"}`.
    pub fn to_json(&self) -> String {
        serde_json::json!({ "code": self.code.as_str(), "message": self.message }).to_string()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
