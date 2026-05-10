use pinocchio::{entrypoint, error::ProgramError, AccountView, Address, ProgramResult};

use crate::instructions::{
    claim_reward::process_claim_reward, enter_pool::process_enter_pool,
    initialize_config::process_initialize_config, initialize_pool::process_initialize_pool,
    lock_pool::process_lock_pool, settle_pool::process_settle_pool,
    submit_scores::process_submit_scores, SenshiInstruction,
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

    let instruction = SenshiInstruction::unpack(instruction_data)?;

    match instruction {
        SenshiInstruction::InitializeConfig => {
            pinocchio_log::log!("Instruction: InitializeConfig");
            process_initialize_config(program_id, accounts)
        }
        SenshiInstruction::InitializePool {
            entry_fee,
            epoch_start,
            epoch_end,
        } => {
            pinocchio_log::log!("Instruction: InitializePool");
            process_initialize_pool(program_id, accounts, entry_fee, epoch_start, epoch_end)
        }
        SenshiInstruction::EnterPool { epoch_start } => {
            pinocchio_log::log!("Instruction: EnterPool");
            process_enter_pool(program_id, accounts, epoch_start)
        }
        SenshiInstruction::LockPool { epoch_start } => {
            pinocchio_log::log!("Instruction: LockPool");
            process_lock_pool(program_id, accounts, epoch_start)
        }
        SenshiInstruction::SubmitScores { epoch_start, score } => {
            pinocchio_log::log!("Instruction: SubmitScores");
            process_submit_scores(program_id, accounts, epoch_start, score)
        }
        SenshiInstruction::SettlePool { epoch_start } => {
            pinocchio_log::log!("Instruction: SettlePool");
            process_settle_pool(program_id, accounts, epoch_start)
        }
        SenshiInstruction::ClaimReward { epoch_start } => {
            pinocchio_log::log!("Instruction: ClaimReward");
            process_claim_reward(program_id, accounts, epoch_start)
        }
    }
}
