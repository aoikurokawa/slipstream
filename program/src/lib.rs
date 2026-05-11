use pinocchio::{entrypoint, error::ProgramError, AccountView, Address, ProgramResult};

use crate::instructions::{
    initialize_config::process_initialize_config, swap::process_swap,
    swap_via_interceptor::process_swap_via_interceptor, SlipstreamInstruction,
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

/// Jito's stake-deposit-interceptor program. Owns custom
/// `stake_deposit_authority` accounts for pools that gate deposits
/// (notably JitoSOL).
const INTERCEPTOR_PROGRAM_ID: Address = Address::new_from_array(pinocchio_pubkey::from_str(
    "5TAiuAh3YGDbwjEruC1ZpXTJWdNDS7Ur7VeqNNiHMmGV",
));

/// Interceptor `DepositStake` instruction tag (borsh u8 enum discriminant).
const TAG_INTERCEPTOR_DEPOSIT_STAKE: u8 = 2;
/// Interceptor `ClaimPoolTokens` instruction tag.
const TAG_INTERCEPTOR_CLAIM_POOL_TOKENS: u8 = 5;

/// Seed prefix for the per-swap transient stake PDA.
const TRANSIENT_STAKE_SEED: &[u8] = b"transient";
/// Seed for the program's global stake authority PDA.
const ROUTER_AUTHORITY_SEED: &[u8] = b"router";

#[repr(u32)]
pub(crate) enum StakeAuthorize {
    Staker = 0,
    Withdrawer = 1,
}

entrypoint!(process_instruction);

pinocchio_pubkey::declare_id!("SL1p2N8iNBBo3uaUF92SGo8VfCkN6Xqdmq7tUTqz6cd");

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
        SlipstreamInstruction::SwapViaInterceptor {
            amount_in,
            min_amount_out,
            nonce,
        } => {
            pinocchio_log::log!("Instruction: SwapViaInterceptor");
            process_swap_via_interceptor(program_id, accounts, amount_in, min_amount_out, nonce)
        }
    }
}
