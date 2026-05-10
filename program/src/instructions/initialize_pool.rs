use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address,
};
use pinocchio_system::instructions::CreateAccount;

use crate::{
    error::SenshiError,
    states::{
        config::Config,
        pool::{Pool, PoolStatus},
    },
};

/// Creates the [`Pool`] PDA and populates it with the provided parameters.
///
/// # Accounts
///
/// 0. `[]` Config PDA.
/// 1. `[writable]` Pool PDA (derived from `["pool", vote_account, epoch]`).
/// 2. `[signer, writable]` Payer — becomes the pool authority.
/// 3. `[]` Vote account.
/// 4. `[]` Vault token account.
/// 5. `[]` System program.
///
/// # Instruction Data (after tag byte)
///
/// | Offset | Size | Field        |
/// |--------|------|--------------|
/// | 0      | 8    | entry_fee    |
/// | 8      | 8    | epoch_start  |
/// | 16     | 8    | epoch_end    |
pub fn process_initialize_pool(
    program_id: &Address,
    accounts: &[AccountView],
    entry_fee: u64,
    epoch_start: u64,
    epoch_end: u64,
) -> Result<(), ProgramError> {
    let [config_view, pool_view, payer_view, vote_account_view, vault_view, system_program_view] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let (config_pubkey, _, _) = Config::find_program_address(program_id);
    if config_pubkey.ne(config_view.address()) {
        pinocchio_log::log!("Config account is not at the correct PDA");
        return Err(ProgramError::InvalidAccountData);
    }

    // Load config
    let config_data = unsafe { config_view.borrow_unchecked() };
    if config_data[0..8] != *Config::DISCRIMINATOR {
        pinocchio_log::log!("Invalid config discriminator");
        return Err(ProgramError::InvalidAccountData);
    }
    let config = unsafe { Config::load_unchecked(&config_data[8..])? };

    if !payer_view.is_signer() {
        pinocchio_log::log!("payer_view is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if config.authority.ne(payer_view.address()) {
        pinocchio_log::log!("Invalid payer");
        return Err(ProgramError::InvalidAccountData);
    }

    if system_program_view.address().ne(&pinocchio_system::id()) {
        pinocchio_log::log!("Account is not the system program");
        return Err(ProgramError::IncorrectProgramId);
    }

    let rent = Rent::get()?;
    let space = 8usize
        .checked_add(Pool::LEN)
        .ok_or(SenshiError::ArithmeticError)?;

    let (pool_pubkey, pool_bump, mut pool_seeds) =
        Pool::find_program_address(program_id, vote_account_view.address(), epoch_start);
    pool_seeds.push(vec![pool_bump]);
    if pool_pubkey.ne(pool_view.address()) {
        pinocchio_log::log!("Pool account is not at the correct PDA");
        return Err(ProgramError::InvalidAccountData);
    }

    let seeds: Vec<Seed> = pool_seeds
        .iter()
        .map(|seed| Seed::from(seed.as_slice()))
        .collect();
    let signers = [Signer::from(seeds.as_slice())];

    pinocchio_log::log!("Initializing Pool at address");
    CreateAccount {
        from: payer_view,
        to: pool_view,
        lamports: rent.minimum_balance_unchecked(space),
        space: space as u64,
        owner: program_id,
    }
    .invoke_signed(&signers)?;

    let pool = unsafe {
        let pool_data = pool_view.borrow_unchecked_mut();
        pool_data[0..8].copy_from_slice(Pool::DISCRIMINATOR);
        Pool::load_mut_unchecked(&mut pool_data[8..])?
    };

    pool.authority = payer_view.address().clone();
    pool.vault = vault_view.address().clone();
    pool.entry_fee = entry_fee;
    pool.status = PoolStatus::Open as u8;
    pool.epoch_start = epoch_start;
    pool.epoch_end = epoch_end;
    pool.total_entries = 0;
    pool.prize_pool = 0;
    pool.bump = pool_bump;
    pool.reserved = [0u8; 128];

    Ok(())
}
