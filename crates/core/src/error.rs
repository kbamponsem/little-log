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
