use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    AccountView, Address,
};
use pinocchio_token::instructions::Transfer;

use crate::{
    error::SenshiError,
    states::{
        entry::Entry,
        pool::{Pool, PoolStatus},
    },
};

/// Claims a player's reward after the pool has been settled.
///
/// Transfers the reward amount from the vault to the player's token account
/// using the vault authority PDA as signer.
///
/// # Accounts
///
/// 0. `[]` Pool PDA.
/// 1. `[writable]` Entry PDA.
/// 2. `[signer]` Player.
/// 3. `[]` Vote account.
/// 4. `[writable]` Vault token account.
/// 5. `[]` Vault authority PDA.
/// 6. `[writable]` Player's JitoSOL token account.
/// 7. `[]` Token program.
///
/// # Instruction Data (after tag byte)
///
/// | Offset | Size | Field       |
/// |--------|------|-------------|
/// | 0      | 8    | epoch_start |
pub fn process_claim_reward(
    program_id: &Address,
    accounts: &[AccountView],
    epoch_start: u64,
) -> Result<(), ProgramError> {
    let [pool_view, entry_view, player_view, vote_account_view, vault_view, vault_authority_view, player_token_view, token_program_view] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !player_view.is_signer() {
        pinocchio_log::log!("player is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if token_program_view.address().ne(&pinocchio_token::id()) {
        pinocchio_log::log!("Account is not the token program");
        return Err(ProgramError::IncorrectProgramId);
    }

    // Verify pool PDA
    let (pool_pubkey, _, _) =
        Pool::find_program_address(program_id, vote_account_view.address(), epoch_start);
    if pool_pubkey.ne(pool_view.address()) {
        pinocchio_log::log!("Pool account is not at the correct PDA");
        return Err(ProgramError::InvalidAccountData);
    }

    // Load pool (read-only)
    let pool_data = unsafe { pool_view.borrow_unchecked_mut() };
    if pool_data[0..8] != *Pool::DISCRIMINATOR {
        pinocchio_log::log!("Invalid pool discriminator");
        return Err(ProgramError::InvalidAccountData);
    }
    let pool = unsafe { Pool::load_mut_unchecked(&mut pool_data[8..])? };

    // Pool must be Settled
    if pool.status != PoolStatus::Settled as u8 {
        pinocchio_log::log!("Pool is not settled");
        return Err(SenshiError::NotSettled.into());
    }

    // Verify vault matches pool
    if vault_view.address().ne(&pool.vault) {
        pinocchio_log::log!("Vault does not match pool vault");
        return Err(ProgramError::InvalidAccountData);
    }

    // Verify entry PDA
    let (entry_pubkey, _, _) =
        Entry::find_program_address(program_id, pool_view.address(), player_view.address());
    if entry_pubkey.ne(entry_view.address()) {
        pinocchio_log::log!("Entry account is not at the correct PDA");
        return Err(ProgramError::InvalidAccountData);
    }

    // Load entry
    let entry_data = unsafe { entry_view.borrow_unchecked_mut() };
    if entry_data[0..8] != *Entry::DISCRIMINATOR {
        pinocchio_log::log!("Invalid entry discriminator");
        return Err(ProgramError::InvalidAccountData);
    }
    let entry = unsafe { Entry::load_mut_unchecked(&mut entry_data[8..])? };

    // Must not have already claimed
    if entry.claimed != 0 {
        pinocchio_log::log!("Reward already claimed");
        return Err(SenshiError::AlreadyClaimed.into());
    }

    // Must have a reward
    if entry.has_reward == 0 {
        pinocchio_log::log!("No reward assigned");
        return Err(SenshiError::NoReward.into());
    }

    let reward = entry.reward;
    entry.claimed = 1;

    // Transfer reward from vault to player's token account using vault authority PDA
    // Vault authority PDA: ["vault", epoch_start, pool_bump]
    let vault_seeds: Vec<Vec<u8>> = vec![
        b"vault".to_vec(),
        epoch_start.to_le_bytes().to_vec(),
        vec![pool.bump],
    ];
    let seeds: Vec<Seed> = vault_seeds
        .iter()
        .map(|seed| Seed::from(seed.as_slice()))
        .collect();
    let signers = [Signer::from(seeds.as_slice())];

    Transfer {
        from: vault_view,
        to: player_token_view,
        authority: vault_authority_view,
        amount: reward,
    }
    .invoke_signed(&signers)?;

    Ok(())
}
