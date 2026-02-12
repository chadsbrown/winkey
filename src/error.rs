//! Error and Result types for the winkey crate.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("transport error: {0}")]
    Transport(String),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("timeout waiting for response")]
    Timeout,

    #[error("unsupported operation: {0}")]
    Unsupported(String),

    #[error("invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("not connected")]
    NotConnected,

    #[error("connection lost")]
    ConnectionLost,

    #[error("buffer full (XOFF)")]
    BufferFull,

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
