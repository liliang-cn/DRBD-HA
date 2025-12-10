use thiserror::Error;

#[derive(Error, Debug)]
pub enum DrbdError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("JSON parsing failed: {0}")]
    JsonParse(String),
}

pub type DrbdResult<T> = Result<T, DrbdError>;
