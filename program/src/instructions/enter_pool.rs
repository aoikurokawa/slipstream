use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address,
};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::instructions::Transfer;

use crate::{
    error::SenshiError,
    states::{
        entry::Entry,
        pool::{Pool, PoolStatus},
    },
};

/// Enters a player into an open per-validator pool.
///
/// Creates the [`Entry`] PDA, transfers the entry fee from the player's token
/// account to the pool vault, and increments the pool's `total_entries`
/// and `prize_pool`.
///
/// # Accounts
///
/// 0. `[writable]` Pool PDA.
/// 1. `[writable]` Entry PDA (derived from `["entry", pool, player]`).
/// 2. `[signer, writable]` Player.
/// 3. `[]` Vote account.
/// 4. `[writable]` Player's JitoSOL token account.
/// 5. `[writable]` Pool vault token account.
/// 6. `[]` Token program.
/// 7. `[]` System program.
///
/// # Instruction Data (after tag byte)
///
/// | Offset | Size | Field       |
/// |--------|------|-------------|
/// | 0      | 8    | epoch_start |
pub fn process_enter_pool(
    program_id: &Address,
    accounts: &[AccountView],
    epoch_start: u64,
) -> Result<(), ProgramError> {
    let [pool_view, entry_view, player_view, vote_account_view, player_token_view, vault_view, token_program_view, system_program_view] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !player_view.is_signer() {
        pinocchio_log::log!("player is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if system_program_view.address().ne(&pinocchio_system::id()) {
        pinocchio_log::log!("Account is not the system program");
        return Err(ProgramError::IncorrectProgramId);
    }

    if token_program_view.address().ne(&pinocchio_token::id()) {
        pinocchio_log::log!("Account is not the token program");
        return Err(ProgramError::IncorrectProgramId);
    }

    // Load pool
    let pool_data = unsafe { pool_view.borrow_unchecked_mut() };
    if pool_data[0..8] != *Pool::DISCRIMINATOR {
        pinocchio_log::log!("Invalid pool discriminator");
        return Err(ProgramError::InvalidAccountData);
    }
    let pool = unsafe { Pool::load_mut_unchecked(&mut pool_data[8..])? };

    // Verify pool PDA
    let (pool_pubkey, _, _) =
        Pool::find_program_address(program_id, vote_account_view.address(), epoch_start);
    if pool_pubkey.ne(pool_view.address()) {
        pinocchio_log::log!("Pool account is not at the correct PDA");
        return Err(ProgramError::InvalidAccountData);
    }

    // Pool must be open
    if pool.status != PoolStatus::Open as u8 {
        pinocchio_log::log!("Pool is not open");
        return Err(SenshiError::PoolNotOpen.into());
    }

    // Verify vault matches pool
    if vault_view.address().ne(&pool.vault) {
        pinocchio_log::log!("Vault does not match pool vault");
        return Err(ProgramError::InvalidAccountData);
    }

    // Transfer entry fee from player's token account to vault
    Transfer {
        from: player_token_view,
        to: vault_view,
        authority: player_view,
        amount: pool.entry_fee,
    }
    .invoke()?;

    // Create Entry PDA
    let (entry_pubkey, entry_bump, mut entry_seeds) =
        Entry::find_program_address(program_id, pool_view.address(), player_view.address());
    entry_seeds.push(vec![entry_bump]);
    if entry_pubkey.ne(entry_view.address()) {
        pinocchio_log::log!("Entry account is not at the correct PDA");
        return Err(ProgramError::InvalidAccountData);
    }

    let rent = Rent::get()?;
    let space = 8usize
        .checked_add(Entry::LEN)
        .ok_or(SenshiError::ArithmeticError)?;

    let seeds: Vec<Seed> = entry_seeds
        .iter()
        .map(|seed| Seed::from(seed.as_slice()))
        .collect();
    let signers = [Signer::from(seeds.as_slice())];

    pinocchio_log::log!("Creating Entry account");
    CreateAccount {
        from: player_view,
        to: entry_view,
        lamports: rent.minimum_balance_unchecked(space),
        space: space as u64,
        owner: program_id,
    }
    .invoke_signed(&signers)?;

    // Initialize entry
    let entry = unsafe {
        let entry_data = entry_view.borrow_unchecked_mut();
        entry_data[0..8].copy_from_slice(Entry::DISCRIMINATOR);
        Entry::load_mut_unchecked(&mut entry_data[8..])?
    };

    entry.player = player_view.address().clone();
    entry.has_score = 0;
    entry.score = 0;
    entry.has_reward = 0;
    entry.reward = 0;
    entry.claimed = 0;
    entry.bump = entry_bump;
    entry.reserved = [0u8; 64];

    // Update pool
    pool.total_entries = pool
        .total_entries
        .checked_add(1)
        .ok_or(SenshiError::ArithmeticError)?;
    pool.prize_pool = pool
        .prize_pool
        .checked_add(pool.entry_fee)
        .ok_or(SenshiError::ArithmeticError)?;

    Ok(())
}
