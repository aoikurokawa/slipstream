use pinocchio::{
    error::ProgramError,
    sysvars::{clock::Clock, Sysvar},
    AccountView, Address,
};

use crate::{
    error::SenshiError,
    states::pool::{Pool, PoolStatus},
};

/// Settles the pool after scoring is complete and the epoch window has ended.
///
/// Updates the prize pool to the current vault balance and transitions the
/// pool to `Settled`, enabling reward claims.
///
/// # Accounts
///
/// 0. `[writable]` Pool PDA.
/// 1. `[signer]` Authority.
/// 2. `[]` Vote account.
/// 3. `[]` Vault token account.
///
/// # Instruction Data (after tag byte)
///
/// | Offset | Size | Field       |
/// |--------|------|-------------|
/// | 0      | 8    | epoch_start |
pub fn process_settle_pool(
    program_id: &Address,
    accounts: &[AccountView],
    epoch_start: u64,
) -> Result<(), ProgramError> {
    let [pool_view, authority_view, vote_account_view, vault_view] = accounts else {
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

    // Verify vault matches pool
    if vault_view.address().ne(&pool.vault) {
        pinocchio_log::log!("Vault does not match pool vault");
        return Err(ProgramError::InvalidAccountData);
    }

    // Pool must be Scoring
    if pool.status != PoolStatus::Scoring as u8 {
        pinocchio_log::log!("Pool is not in Scoring status");
        return Err(SenshiError::InvalidTransition.into());
    }

    // Current epoch must be past epoch_end
    let clock = Clock::get()?;
    if clock.epoch <= pool.epoch_end {
        pinocchio_log::log!("Pool epoch has not ended");
        return Err(SenshiError::EpochNotEnded.into());
    }

    // Update prize pool from vault balance (includes accrued yield)
    let vault_data = vault_view.try_borrow()?;
    // SPL Token account: amount is at offset 64, 8 bytes little-endian
    let vault_amount = u64::from_le_bytes(vault_data[64..72].try_into().unwrap());
    pool.prize_pool = vault_amount;

    pool.status = PoolStatus::Settled as u8;

    Ok(())
}
