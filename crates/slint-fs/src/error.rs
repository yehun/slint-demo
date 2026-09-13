use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[cfg(target_os = "android")]
    #[error("jni error: {0}")]
    Jni(#[from] jni::errors::Error),
    #[error("operation not supported on content URI")]
    NotSupported,
    #[error("invalid URI")]
    InvalidUri,
    #[error("Illegal mode: {0}")]
    AccessError(String),
}

pub type Result<T> = std::result::Result<T, Error>;
