use pinocchio::{entrypoint, error::ProgramError, AccountView, Address, ProgramResult};

use crate::instructions::{
    initialize_config::process_initialize_config, swap::process_swap, SlipstreamInstruction,
};

pub mod error;
pub mod instructions;
pub mod states;

/// Native stake program: `Stake11111111111111111111111111111111111111`.
const STAKE_PROGRAM_ID: Address = Address::new_from_array(pinocchio_pubkey::from_str(
    "Stake11111111111111111111111111111111111111",
));

/// Size of `StakeStateV2` (4-byte enum tag + Meta(120) + Stake(72) + flags(4)).
const STAKE_ACCOUNT_SIZE: u64 = 200;

/// SPL stake-pool `WithdrawStake` instruction tag.
const TAG_WITHDRAW_STAKE: u8 = 10;
/// SPL stake-pool `DepositStake` instruction tag.
const TAG_DEPOSIT_STAKE: u8 = 9;

/// Stake program `Authorize` instruction tag (bincode `u32` LE).
const STAKE_AUTHORIZE_TAG: u32 = 1;

/// Seed prefix for the per-swap transient stake PDA.
const TRANSIENT_STAKE_SEED: &[u8] = b"transient";
/// Seed for the program's global stake authority PDA.
const ROUTER_AUTHORITY_SEED: &[u8] = b"router";

#[repr(u32)]
enum StakeAuthorize {
    Staker = 0,
    Withdrawer = 1,
}

entrypoint!(process_instruction);

pinocchio_pubkey::declare_id!("SenPmWgTAKKhCxCAtKJLkV5yz7YW8VKQgUpTE5rEFYb");

fn process_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    if program_id.ne(&Address::new_from_array(id())) {
        return Err(ProgramError::IncorrectProgramId);
    }

    let instruction = SlipstreamInstruction::unpack(instruction_data)?;

    match instruction {
        SlipstreamInstruction::InitializeConfig => {
            pinocchio_log::log!("Instruction: InitializeConfig");
            process_initialize_config(program_id, accounts)
        }
        SlipstreamInstruction::Swap {
            amount_in,
            min_amount_out,
            nonce,
        } => {
            pinocchio_log::log!("Instruction: Swap");
            process_swap(program_id, accounts, amount_in, min_amount_out, nonce)
        }
    }
}
