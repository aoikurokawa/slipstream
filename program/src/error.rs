use pinocchio::error::ProgramError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SenshiError {
    #[error("Encountered an arithmetic under/overflow error.")]
    ArithmeticError,

    #[error("Pool is not open for entries.")]
    PoolNotOpen,

    #[error("Invalid pool status transition.")]
    InvalidTransition,

    #[error("Target epoch has not been reached yet.")]
    EpochNotReached,

    #[error("Pool epoch has not ended yet.")]
    EpochNotEnded,

    #[error("Pool is not settled.")]
    NotSettled,

    #[error("Reward has already been claimed.")]
    AlreadyClaimed,

    #[error("No reward assigned to this entry.")]
    NoReward,
}

impl From<SenshiError> for ProgramError {
    fn from(value: SenshiError) -> Self {
        Self::Custom(value as u32)
    }
}
