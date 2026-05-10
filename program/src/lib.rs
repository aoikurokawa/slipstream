use pinocchio::{entrypoint, error::ProgramError, AccountView, Address, ProgramResult};

use crate::instructions::{
    initialize_config::process_initialize_config, swap::process_swap, SlipstreamInstruction,
};

pub mod error;
pub mod instructions;
pub mod states;

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
