//! CPI helpers shared between `Swap` and `SwapViaInterceptor`. Both routes
//! burn LST_A and split a stake account; only the deposit-side differs.

use pinocchio::{
    cpi::{invoke, invoke_signed, Signer},
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    AccountView, Address,
};

use crate::{StakeAuthorize, STAKE_AUTHORIZE_TAG, STAKE_PROGRAM_ID, TAG_WITHDRAW_STAKE};

/// CPI into an SPL stake-pool program's `WithdrawStake` instruction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn withdraw_stake(
    pool_program_id: &Address,
    pool: &AccountView,
    validator_list: &AccountView,
    pool_withdraw_authority: &AccountView,
    validator_stake: &AccountView,
    transient_stake: &AccountView,
    new_stake_authority: &AccountView,
    user_transfer_authority: &AccountView,
    user_pool_tokens: &AccountView,
    manager_fee: &AccountView,
    pool_mint: &AccountView,
    clock: &AccountView,
    token_program: &AccountView,
    stake_program: &AccountView,
    amount: u64,
) -> Result<(), ProgramError> {
    let mut data = [0u8; 9];
    data[0] = TAG_WITHDRAW_STAKE;
    data[1..9].copy_from_slice(&amount.to_le_bytes());

    let metas = [
        InstructionAccount::writable(pool.address()),
        InstructionAccount::writable(validator_list.address()),
        InstructionAccount::readonly(pool_withdraw_authority.address()),
        InstructionAccount::writable(validator_stake.address()),
        InstructionAccount::writable(transient_stake.address()),
        InstructionAccount::readonly(new_stake_authority.address()),
        InstructionAccount::readonly_signer(user_transfer_authority.address()),
        InstructionAccount::writable(user_pool_tokens.address()),
        InstructionAccount::writable(manager_fee.address()),
        InstructionAccount::writable(pool_mint.address()),
        InstructionAccount::readonly(clock.address()),
        InstructionAccount::readonly(token_program.address()),
        InstructionAccount::readonly(stake_program.address()),
    ];

    let ix = InstructionView {
        program_id: pool_program_id,
        accounts: &metas,
        data: &data,
    };

    invoke(
        &ix,
        &[
            pool,
            validator_list,
            pool_withdraw_authority,
            validator_stake,
            transient_stake,
            new_stake_authority,
            user_transfer_authority,
            user_pool_tokens,
            manager_fee,
            pool_mint,
            clock,
            token_program,
            stake_program,
        ],
    )
}

/// CPI the native stake program's `Authorize` instruction to transfer the
/// `Staker` or `Withdrawer` authority of a stake account.
pub(crate) fn stake_authorize(
    stake: &AccountView,
    clock: &AccountView,
    authority: &AccountView,
    new_authority: &Address,
    kind: StakeAuthorize,
    signers: &[Signer],
) -> Result<(), ProgramError> {
    // bincode: u32 tag, 32-byte pubkey, u32 enum variant.
    let mut data = [0u8; 40];
    data[0..4].copy_from_slice(&STAKE_AUTHORIZE_TAG.to_le_bytes());
    data[4..36].copy_from_slice(new_authority.as_ref());
    data[36..40].copy_from_slice(&(kind as u32).to_le_bytes());

    let metas = [
        InstructionAccount::writable(stake.address()),
        InstructionAccount::readonly(clock.address()),
        InstructionAccount::readonly_signer(authority.address()),
    ];

    let ix = InstructionView {
        program_id: &STAKE_PROGRAM_ID,
        accounts: &metas,
        data: &data,
    };

    invoke_signed(&ix, &[stake, clock, authority], signers)
}
