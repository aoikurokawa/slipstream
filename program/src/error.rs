use pinocchio::error::ProgramError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SlipstreamError {
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

    #[error("Output amount is below the requested minimum.")]
    SlippageExceeded,
}

impl From<SlipstreamError> for ProgramError {
    fn from(value: SlipstreamError) -> Self {
        Self::Custom(value as u32)
    }
}
