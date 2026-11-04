//! Error type shared by the whole crate.

use std::path::Path;

/// Errors returned by the imaging engine. Messages are written so they can be
/// shown to users as-is.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be read or written.
    #[error("{path}: {source}")]
    File {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// Generic I/O failure without a known path.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The file is not an image format we recognise.
    #[error("Unrecognized image format{0}")]
    UnknownFormat(String),

    /// The operation is not supported (e.g. writing HEIC).
    #[error("{0}")]
    Unsupported(String),

    /// The image data is corrupt or could not be decoded.
    #[error("Could not decode {format} image: {message}")]
    Decode {
        format: &'static str,
        message: String,
    },

    /// The encoder failed.
    #[error("Could not encode {format} image: {message}")]
    Encode {
        format: &'static str,
        message: String,
    },

    /// Metadata could not be read or written.
    #[error("Metadata error: {0}")]
    Metadata(String),

    /// A parameter was out of range.
    #[error("Invalid parameter: {0}")]
    InvalidInput(String),
}

/// Crate-wide result alias.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn file(path: &Path, source: std::io::Error) -> Self {
        Error::File {
            path: path.display().to_string(),
            source,
        }
    }

    pub(crate) fn decode(format: &'static str, message: impl ToString) -> Self {
        Error::Decode {
            format,
            message: message.to_string(),
        }
    }

    pub(crate) fn encode(format: &'static str, message: impl ToString) -> Self {
        Error::Encode {
            format,
            message: message.to_string(),
        }
    }
}

/// Run `f`, converting a panic inside third-party codec code into an error.
///
/// Several codecs (libjpeg via `mozjpeg`, some pure-Rust decoders on hostile
/// input) report fatal errors by unwinding. We never want a corrupt file to
/// take down a worker thread, so every codec entry point goes through here.
pub(crate) fn guard<T>(
    what: &'static str,
    decode: bool,
    f: impl FnOnce() -> Result<T>,
) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = payload.downcast_ref::<&str>() {
                (*s).to_string()
            } else {
                "internal codec error".to_string()
            };
            Err(if decode {
                Error::decode(what, msg)
            } else {
                Error::encode(what, msg)
            })
        }
    }
}
