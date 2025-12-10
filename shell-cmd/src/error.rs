use thiserror::Error;

#[derive(Error, Debug)]
pub enum ShellError {
    #[error("Command execution failed: {0}")]
    Execution(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type ShellResult<T> = Result<T, ShellError>;
