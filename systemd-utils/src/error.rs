use thiserror::Error;

#[derive(Error, Debug)]
pub enum SystemdError {
    #[error("D-Bus error: {0}")]
    DBus(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Remote execution error: {0}")]
    RemoteExecution(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Other error: {0}")]
    Other(String),
}

pub type SystemdResult<T> = Result<T, SystemdError>;
