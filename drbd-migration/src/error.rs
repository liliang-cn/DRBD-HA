use thiserror::Error;

pub type MigrationResult<T> = Result<T, MigrationError>;

#[derive(Error, Debug)]
pub enum MigrationError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Command execution failed: {0}")]
    Command(String),

    #[error("DRBD error: {0}")]
    Drbd(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Systemd error: {0}")]
    Systemd(#[from] systemd_utils::SystemdError),

    #[error("Unknown error: {0}")]
    Unknown(String),
}
