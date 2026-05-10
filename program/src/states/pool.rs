use pinocchio::{error::ProgramError, Address};

use crate::error::SenshiError;

/// Represents the lifecycle status of a pool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PoolStatus {
    /// Pool is accepting new entries.
    Open = 0,

    /// Entries are locked; no new participants can join.
    Locked = 1,

    /// Epoch has ended and scores are being calculated.
    Scoring = 2,

    /// Rewards have been distributed and the pool is complete.
    Settled = 3,
}

impl TryFrom<u8> for PoolStatus {
    type Error = ProgramError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PoolStatus::Open),
            1 => Ok(PoolStatus::Locked),
            2 => Ok(PoolStatus::Scoring),
            3 => Ok(PoolStatus::Settled),
            _ => Err(ProgramError::InvalidAccountData),
        }
    }
}

/// On-chain state for a single per-validator pool.
///
/// A pool defines the epoch range during which participants stake on a
/// vote account and compete for a share of the prize pool. The lifecycle
/// flows through `Open -> Locked -> Scoring -> Settled`.
#[derive(Debug)]
#[repr(C)]
pub struct Pool {
    /// Authority that can manage this pool (lock, score, settle).
    pub authority: Address,

    /// Token account that holds collected entry fees and prizes.
    pub vault: Address,

    /// First epoch (inclusive) of the scoring window.
    pub epoch_start: u64,

    /// Last epoch (inclusive) of the scoring window.
    pub epoch_end: u64,

    /// Accumulated prize pool in lamports.
    pub prize_pool: u64,

    /// Lamports required to submit an entry.
    pub entry_fee: u64,

    /// Number of entries submitted so far.
    pub total_entries: u32,

    /// Current lifecycle status (see [`PoolStatus`]).
    pub status: u8,

    /// PDA bump seed.
    pub bump: u8,

    /// Reserved space for future fields.
    pub reserved: [u8; 128],
}

impl Pool {
    // 32 + 32 + 8 + 8 + 8 + 8 + 4 + 1 + 1 + 128 = 230
    pub const LEN: usize = 230;
    pub const DISCRIMINATOR: &'static [u8] = &[2, 0, 0, 0, 0, 0, 0, 0];

    /// Return a mutable `Pool` reference from the given bytes.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `bytes` contains a valid representation of `Pool`.
    #[inline(always)]
    pub unsafe fn load_mut_unchecked(bytes: &mut [u8]) -> Result<&mut Self, ProgramError> {
        if bytes.len()
            != Self::LEN
                .checked_sub(8)
                .ok_or(SenshiError::ArithmeticError)?
        {
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(&mut *(bytes.as_mut_ptr() as *mut Self))
    }

    /// Returns the seeds for the PDA: `["pool", vote_account, epoch]`
    pub fn seeds(vote_account: &Address, epoch: u64) -> Vec<Vec<u8>> {
        vec![
            b"pool".to_vec(),
            vote_account.as_ref().to_vec(),
            epoch.to_be_bytes().to_vec(),
        ]
    }

    /// Find the program address for the pool account (per vote_account + epoch)
    #[inline(always)]
    pub fn find_program_address(
        program_id: &Address,
        vote_account: &Address,
        epoch: u64,
    ) -> (Address, u8, Vec<Vec<u8>>) {
        let seeds = Self::seeds(vote_account, epoch);
        let seeds_iter: Vec<&[u8]> = seeds.iter().map(|s| s.as_slice()).collect();
        let (pda, bump) = Address::find_program_address(&seeds_iter, program_id);
        (pda, bump, seeds)
    }
}
