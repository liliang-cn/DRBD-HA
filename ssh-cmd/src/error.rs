use thiserror::Error;

#[derive(Error, Debug)]
pub enum SshError {
    #[error("SSH command execution failed: {0}")]
    Execution(String),

    #[error("Connection timeout: {0}")]
    Timeout(String),

    #[error("JSON parsing failed: {0}")]
    JsonParse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Encoding error: {0}")]
    Encoding(String),
}

pub type SshResult<T> = Result<T, SshError>;
