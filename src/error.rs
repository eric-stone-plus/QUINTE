use thiserror::Error;

#[derive(Debug, Error)]
pub enum QuinteError {
    #[error("{0}")]
    Usage(String),
    #[error("{0}")]
    Runtime(String),
    #[error("{0}")]
    Policy(String),
    #[error("run was cancelled")]
    Cancelled,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl QuinteError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Runtime(_) => 1,
            Self::Usage(_) => 2,
            Self::Policy(_) => 3,
            Self::Cancelled => 4,
            Self::Internal(_) => 1,
        }
    }
}

pub type Result<T> = std::result::Result<T, QuinteError>;
