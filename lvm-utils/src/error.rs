use thiserror::Error;

#[derive(Error, Debug)]
pub enum LvmError {
    #[error("LVM command execution failed: {0}")]
    Execution(String),

    #[error("JSON parsing failed: {0}")]
    JsonParse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("SSH error: {0}")]
    Ssh(#[from] ssh_cmd::error::SshError),

    #[error("Configuration error: {0}")]
    Config(String),
}

pub type LvmResult<T> = Result<T, LvmError>;
