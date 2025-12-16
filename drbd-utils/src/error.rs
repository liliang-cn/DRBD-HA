use thiserror::Error;

#[derive(Error, Debug)]
pub enum DrbdError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("JSON parsing failed: {0}")]
    JsonParse(String),

    #[error("Command execution error: {0}")]
    Command(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type DrbdResult<T> = Result<T, DrbdError>;

impl From<shell_cmd::error::ShellError> for DrbdError {
    fn from(err: shell_cmd::error::ShellError) -> Self {
        DrbdError::Command(err.to_string())
    }
}
