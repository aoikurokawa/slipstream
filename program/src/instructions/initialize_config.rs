use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address,
};
use pinocchio_system::instructions::CreateAccount;

use crate::{error::SenshiError, states::config::Config};

pub fn process_initialize_config(
    program_id: &Address,
    accounts: &[AccountView],
) -> Result<(), ProgramError> {
    let [config_view, payer_view, system_program_view] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !payer_view.is_signer() {
        pinocchio_log::log!("payer_view is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }

    if system_program_view.address().ne(&pinocchio_system::id()) {
        pinocchio_log::log!("Account is not the system program");
        return Err(ProgramError::IncorrectProgramId);
    }

    let rent = Rent::get()?;
    let space = 8usize
        .checked_add(Config::LEN)
        .ok_or(SenshiError::ArithmeticError)?;

    let (config_pubkey, config_bump, mut config_seeds) = Config::find_program_address(program_id);
    config_seeds.push(vec![config_bump]);
    if config_pubkey.ne(config_view.address()) {
        pinocchio_log::log!("Config account is not at the correct PDA");
        return Err(ProgramError::InvalidAccountData);
    }

    let seeds: Vec<Seed> = config_seeds
        .iter()
        .map(|seed| Seed::from(seed.as_slice()))
        .collect();
    let signers = [Signer::from(seeds.as_slice())];

    pinocchio_log::log!("Initializing Config at address");
    CreateAccount {
        from: payer_view,
        to: config_view,
        lamports: rent.minimum_balance_unchecked(space),
        space: space as u64,
        owner: program_id,
    }
    .invoke_signed(&signers)?;

    let config = unsafe {
        let config_data = config_view.borrow_unchecked_mut();
        config_data[0..8].copy_from_slice(Config::DISCRIMINATOR);
        Config::load_mut_unchecked(&mut config_data[8..])?
    };

    config.authority = payer_view.address().clone();

    Ok(())
}
