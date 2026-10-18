use thiserror::Error;

/// Anything the core can fail with.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("could not read or write {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid data: {0}")]
    Data(String),

    #[error("unknown {enum_name} value \"{value}\"")]
    UnknownEnum {
        enum_name: &'static str,
        value: String,
    },

    #[error("{0}")]
    Invalid(String),

    #[error("entry {0} was not found")]
    NotFound(String),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    /// AEAD open failed: wrong group key, wrong associated data, or a tampered
    /// ciphertext. Deliberately opaque — a caller learns nothing about which.
    #[error("payload could not be opened")]
    Unseal,

    /// An event's signature did not verify against the key its author names.
    #[error("event signature did not verify")]
    BadSignature,

    /// The operating system's random source failed.
    #[error("could not read random bytes: {0}")]
    Random(String),
}

impl CoreError {
    pub fn data(message: impl Into<String>) -> Self {
        CoreError::Data(message.into())
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        CoreError::Invalid(message.into())
    }

    pub fn io(path: impl Into<String>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
