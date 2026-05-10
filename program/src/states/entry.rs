use pinocchio::{error::ProgramError, Address};

use crate::error::SenshiError;

/// On-chain state for a player's entry in a season.
///
/// Each entry represents a participant's stake in a per-validator season.
/// The PDA is derived from `["entry", season_pda, player]`.
#[derive(Debug)]
#[repr(C)]
pub struct Entry {
    /// The player who submitted this entry.
    pub player: Address,

    /// Whether the score has been set (0 = no, 1 = yes).
    pub has_score: u8,

    /// The computed score (only valid if `has_score == 1`).
    pub score: u64,

    /// Whether the reward has been set (0 = no, 1 = yes).
    pub has_reward: u8,

    /// The reward amount in lamports (only valid if `has_reward == 1`).
    pub reward: u64,

    /// Whether the reward has been claimed.
    pub claimed: u8,

    /// PDA bump seed.
    pub bump: u8,

    /// Reserved space for future fields.
    pub reserved: [u8; 64],
}

impl Entry {
    // 32 + 1 + 8 + 1 + 8 + 1 + 1 + 64 = 116
    pub const LEN: usize = 116;
    pub const DISCRIMINATOR: &'static [u8] = &[3, 0, 0, 0, 0, 0, 0, 0];

    /// Return a mutable `Entry` reference from the given bytes.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `bytes` contains a valid representation of `Entry`.
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

    /// Returns the seeds for the PDA
    pub fn seeds(season_pda: &Address, player: &Address) -> Vec<Vec<u8>> {
        vec![
            b"entry".to_vec(),
            season_pda.as_ref().to_vec(),
            player.as_ref().to_vec(),
        ]
    }

    /// Find the program address for an entry account
    #[inline(always)]
    pub fn find_program_address(
        program_id: &Address,
        season_pda: &Address,
        player: &Address,
    ) -> (Address, u8, Vec<Vec<u8>>) {
        let seeds = Self::seeds(season_pda, player);
        let seeds_iter: Vec<&[u8]> = seeds.iter().map(|s| s.as_slice()).collect();
        let (pda, bump) = Address::find_program_address(&seeds_iter, program_id);
        (pda, bump, seeds)
    }
}
