use pinocchio::{error::ProgramError, AccountView, Address};

use crate::{
    error::SenshiError,
    states::{
        entry::Entry,
        pool::{Pool, PoolStatus},
    },
};

/// Submits a score for a single entry in a locked or scoring pool.
///
/// The authority calls this once per entry. The pool transitions
/// to `Scoring` on the first call.
///
/// # Accounts
///
/// 0. `[writable]` Pool PDA.
/// 1. `[signer]` Authority.
/// 2. `[]` Vote account.
/// 3. `[writable]` Entry PDA.
///
/// # Instruction Data (after tag byte)
///
/// | Offset | Size | Field       |
/// |--------|------|-------------|
/// | 0      | 8    | epoch_start |
/// | 8      | 8    | score       |
pub fn process_submit_scores(
    program_id: &Address,
    accounts: &[AccountView],
    epoch_start: u64,
    score: u64,
) -> Result<(), ProgramError> {
    let [pool_view, authority_view, vote_account_view, entry_view] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !authority_view.is_signer() {
        pinocchio_log::log!("authority is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Verify pool PDA
    let (pool_pubkey, _, _) =
        Pool::find_program_address(program_id, vote_account_view.address(), epoch_start);
    if pool_pubkey.ne(pool_view.address()) {
        pinocchio_log::log!("Pool account is not at the correct PDA");
        return Err(ProgramError::InvalidAccountData);
    }

    // Load pool
    let pool_data = unsafe { pool_view.borrow_unchecked_mut() };
    if pool_data[0..8] != *Pool::DISCRIMINATOR {
        pinocchio_log::log!("Invalid pool discriminator");
        return Err(ProgramError::InvalidAccountData);
    }
    let pool = unsafe { Pool::load_mut_unchecked(&mut pool_data[8..])? };

    // Verify authority
    if authority_view.address().ne(&pool.authority) {
        pinocchio_log::log!("Authority does not match pool authority");
        return Err(ProgramError::MissingRequiredSignature);
    }

    // Pool must be Locked or Scoring
    if pool.status != PoolStatus::Locked as u8 && pool.status != PoolStatus::Scoring as u8 {
        pinocchio_log::log!("Pool is not in Locked or Scoring status");
        return Err(SenshiError::InvalidTransition.into());
    }

    // Transition to Scoring
    pool.status = PoolStatus::Scoring as u8;

    // Load entry and write score
    let entry_data = unsafe { entry_view.borrow_unchecked_mut() };
    if entry_data[0..8] != *Entry::DISCRIMINATOR {
        pinocchio_log::log!("Invalid entry discriminator");
        return Err(ProgramError::InvalidAccountData);
    }
    let entry = unsafe { Entry::load_mut_unchecked(&mut entry_data[8..])? };

    entry.has_score = 1;
    entry.score = score;

    Ok(())
}
