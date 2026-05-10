use pinocchio::{
    error::ProgramError,
    sysvars::{clock::Clock, Sysvar},
    AccountView, Address,
};

use crate::{
    error::SenshiError,
    states::pool::{Pool, PoolStatus},
};

/// Locks the pool once the target epoch has begun, preventing new entries.
///
/// Only the pool authority can invoke this. The pool must be in `Open`
/// status and the current clock epoch must be >= `epoch_start`.
///
/// # Accounts
///
/// 0. `[writable]` Pool PDA.
/// 1. `[signer]` Authority.
/// 2. `[]` Vote account.
///
/// # Instruction Data (after tag byte)
///
/// | Offset | Size | Field       |
/// |--------|------|-------------|
/// | 0      | 8    | epoch_start |
pub fn process_lock_pool(
    program_id: &Address,
    accounts: &[AccountView],
    epoch_start: u64,
) -> Result<(), ProgramError> {
    let [pool_view, authority_view, vote_account_view] = accounts else {
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

    // Pool must be Open
    if pool.status != PoolStatus::Open as u8 {
        pinocchio_log::log!("Pool is not open");
        return Err(SenshiError::InvalidTransition.into());
    }

    // Current epoch must be >= epoch_start
    let clock = Clock::get()?;
    if clock.epoch < pool.epoch_start {
        pinocchio_log::log!("Target epoch has not been reached");
        return Err(SenshiError::EpochNotReached.into());
    }

    pool.status = PoolStatus::Locked as u8;

    Ok(())
}
