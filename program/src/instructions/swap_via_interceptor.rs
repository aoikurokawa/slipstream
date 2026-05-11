//! Routed swap for pools whose `stake_deposit_authority` is owned by Jito's
//! stake-deposit-interceptor program (notably JitoSOL).
//!
//! Flow:
//!   1. allocate transient stake PDA
//!   2. `WithdrawStake` from pool A
//!   3. `Authorize` × 2 → pool B's deposit authority (`StakePoolDepositStakeAuthority` PDA)
//!   4. `interceptor::DepositStakeWithSlippage` → mints pool tokens to the
//!      interceptor's vault and creates a `DepositReceipt` PDA owned by the
//!      user. Slippage is enforced by the interceptor at this step.
//!
//! We deliberately do NOT call `ClaimPoolTokens` here. After the cool-down
//! period (~10 min), a keeper redeems the receipt → user's LST_B token
//! account. Pulling the claim into the same transaction would force the user
//! to pay the cool-down fee, which is exactly what the keeper-deferred
//! design avoids.

use pinocchio::{
    cpi::{invoke, Seed, Signer},
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address,
};
use pinocchio_system::instructions::CreateAccount;

use crate::{
    instructions::route_common::{stake_authorize, withdraw_stake},
    StakeAuthorize, INTERCEPTOR_PROGRAM_ID, ROUTER_AUTHORITY_SEED, STAKE_ACCOUNT_SIZE,
    STAKE_PROGRAM_ID, TRANSIENT_STAKE_SEED,
};

/// Borsh enum discriminant for
/// `StakeDepositInterceptorInstruction::DepositStakeWithSlippage`.
const TAG_DEPOSIT_STAKE_WITH_SLIPPAGE: u8 = 3;

/// Routed swap LST_A → DepositReceipt (user) when pool B's deposit auth is
/// interceptor-gated. The keeper later claims the receipt into the user's
/// LST_B token account.
///
/// # Accounts
///
///  0. `[signer, writable]`  User (payer + LST_A transfer authority + receipt owner).
///  1. `[writable]`          User's LST_A token account.
///  2. `[writable]`          Transient stake PDA.
///  3. `[]`                  Router authority PDA.
///  4. `[]`                  Pool A's SPL stake-pool program.
///  5. `[writable]`          Pool A state account.
///  6. `[writable]`          Pool A validator list.
///  7. `[]`                  Pool A withdraw authority.
///  8. `[writable]`          Pool A validator stake (split source).
///  9. `[writable]`          Pool A manager fee token account.
/// 10. `[writable]`          Pool A pool mint (LST_A).
/// 11. `[]`                  Pool B's SPL stake-pool program.
/// 12. `[writable]`          Pool B state account.
/// 13. `[writable]`          Pool B validator list.
/// 14. `[]`                  Pool B deposit authority (`StakePoolDepositStakeAuthority` PDA).
/// 15. `[]`                  Pool B withdraw authority.
/// 16. `[writable]`          Pool B validator stake.
/// 17. `[writable]`          Pool B reserve stake account.
/// 18. `[writable]`          Pool B manager fee token account.
/// 19. `[writable]`          Pool B referral fee token account.
/// 20. `[writable]`          Pool B pool mint (LST_B).
/// 21. `[]`                  Stake program.
/// 22. `[]`                  Token program.
/// 23. `[]`                  System program.
/// 24. `[]`                  Clock sysvar.
/// 25. `[]`                  Stake history sysvar.
/// 26. `[]`                  Interceptor program (`5TAiuAh3YG…`).
/// 27. `[writable]`          DepositReceipt PDA.
/// 28. `[signer]`            Ephemeral `base` keypair — seeds the DepositReceipt PDA.
/// 29. `[writable]`          Interceptor vault token account.
///
/// # Instruction Data (after tag byte)
///
/// | Offset | Size | Field          |
/// |--------|------|----------------|
/// | 0      | 8    | amount_in      |
/// | 8      | 8    | min_amount_out |
/// | 16     | 8    | nonce          |
pub fn process_swap_via_interceptor(
    program_id: &Address,
    accounts: &[AccountView],
    amount_in: u64,
    min_amount_out: u64,
    nonce: u64,
) -> Result<(), ProgramError> {
    let [user, user_lst_a, transient_stake, router_authority, pool_a_program, pool_a, pool_a_validator_list, pool_a_withdraw_authority, pool_a_validator_stake, pool_a_manager_fee, pool_a_mint, pool_b_program, pool_b, pool_b_validator_list, pool_b_deposit_authority, pool_b_withdraw_authority, pool_b_validator_stake, pool_b_reserve_stake, pool_b_manager_fee, pool_b_referral_fee, pool_b_mint, stake_program, token_program, system_program, clock, stake_history, interceptor_program, deposit_receipt, base, vault] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !user.is_signer() {
        pinocchio_log::log!("user is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }
    if !base.is_signer() {
        pinocchio_log::log!("base is not a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }
    if system_program.address().ne(&pinocchio_system::id()) {
        return Err(ProgramError::IncorrectProgramId);
    }
    if token_program.address().ne(&pinocchio_token::id()) {
        return Err(ProgramError::IncorrectProgramId);
    }
    if stake_program.address().ne(&STAKE_PROGRAM_ID) {
        return Err(ProgramError::IncorrectProgramId);
    }
    if interceptor_program.address().ne(&INTERCEPTOR_PROGRAM_ID) {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Verify the two slipstream-controlled PDAs.
    let nonce_bytes = nonce.to_le_bytes();
    let user_addr_bytes: &[u8] = user.address().as_ref();
    let (transient_pda, transient_bump) = Address::find_program_address(
        &[TRANSIENT_STAKE_SEED, user_addr_bytes, &nonce_bytes],
        program_id,
    );
    if transient_pda.ne(transient_stake.address()) {
        pinocchio_log::log!("transient_stake is not the expected PDA");
        return Err(ProgramError::InvalidAccountData);
    }
    let (router_pda, router_bump) =
        Address::find_program_address(&[ROUTER_AUTHORITY_SEED], program_id);
    if router_pda.ne(router_authority.address()) {
        pinocchio_log::log!("router_authority is not the expected PDA");
        return Err(ProgramError::InvalidAccountData);
    }

    // ---- 1. allocate transient stake PDA. ----
    let rent = Rent::get()?;
    let transient_lamports = rent.minimum_balance_unchecked(STAKE_ACCOUNT_SIZE as usize);
    let transient_bump_arr = [transient_bump];
    let transient_seeds = [
        Seed::from(TRANSIENT_STAKE_SEED),
        Seed::from(user_addr_bytes),
        Seed::from(nonce_bytes.as_slice()),
        Seed::from(transient_bump_arr.as_slice()),
    ];
    CreateAccount {
        from: user,
        to: transient_stake,
        lamports: transient_lamports,
        space: STAKE_ACCOUNT_SIZE,
        owner: &STAKE_PROGRAM_ID,
    }
    .invoke_signed(&[Signer::from(&transient_seeds)])?;

    // ---- 2. WithdrawStake from pool A. ----
    withdraw_stake(
        pool_a_program.address(),
        pool_a,
        pool_a_validator_list,
        pool_a_withdraw_authority,
        pool_a_validator_stake,
        transient_stake,
        router_authority,
        user,
        user_lst_a,
        pool_a_manager_fee,
        pool_a_mint,
        clock,
        token_program,
        stake_program,
        amount_in,
    )?;

    // ---- 3. Authorize router → pool B deposit authority (Staker, Withdrawer). ----
    let router_bump_arr = [router_bump];
    let router_seeds = [
        Seed::from(ROUTER_AUTHORITY_SEED),
        Seed::from(router_bump_arr.as_slice()),
    ];
    stake_authorize(
        transient_stake,
        clock,
        router_authority,
        pool_b_deposit_authority.address(),
        StakeAuthorize::Staker,
        &[Signer::from(&router_seeds)],
    )?;
    stake_authorize(
        transient_stake,
        clock,
        router_authority,
        pool_b_deposit_authority.address(),
        StakeAuthorize::Withdrawer,
        &[Signer::from(&router_seeds)],
    )?;

    // ---- 4. interceptor::DepositStakeWithSlippage → receipt + vault tokens. ----
    interceptor_deposit_stake_with_slippage(
        interceptor_program.address(),
        user,
        pool_b_program,
        deposit_receipt,
        pool_b,
        pool_b_validator_list,
        pool_b_deposit_authority,
        base,
        pool_b_withdraw_authority,
        transient_stake,
        pool_b_validator_stake,
        pool_b_reserve_stake,
        vault,
        pool_b_manager_fee,
        pool_b_referral_fee,
        pool_b_mint,
        clock,
        stake_history,
        token_program,
        stake_program,
        system_program,
        user.address(),
        min_amount_out,
    )?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn interceptor_deposit_stake_with_slippage(
    interceptor_program_id: &Address,
    payer: &AccountView,
    stake_pool_program: &AccountView,
    deposit_receipt: &AccountView,
    stake_pool: &AccountView,
    validator_stake_list: &AccountView,
    deposit_stake_authority: &AccountView,
    base: &AccountView,
    stake_pool_withdraw_authority: &AccountView,
    stake: &AccountView,
    validator_stake_account: &AccountView,
    reserve_stake_account: &AccountView,
    vault: &AccountView,
    manager_fee_account: &AccountView,
    referrer_pool_tokens_account: &AccountView,
    pool_mint: &AccountView,
    clock: &AccountView,
    stake_history: &AccountView,
    token_program: &AccountView,
    stake_program: &AccountView,
    system_program: &AccountView,
    owner: &Address,
    minimum_pool_tokens_out: u64,
) -> Result<(), ProgramError> {
    // Borsh: u8 enum tag = 3, then `DepositStakeWithSlippageArgs`:
    //   owner: Pubkey (32 bytes)
    //   minimum_pool_tokens_out: u64 (8 bytes)
    let mut data = [0u8; 41];
    data[0] = TAG_DEPOSIT_STAKE_WITH_SLIPPAGE;
    data[1..33].copy_from_slice(owner.as_ref());
    data[33..41].copy_from_slice(&minimum_pool_tokens_out.to_le_bytes());

    let metas = [
        InstructionAccount::writable_signer(payer.address()),
        InstructionAccount::readonly(stake_pool_program.address()),
        InstructionAccount::writable(deposit_receipt.address()),
        InstructionAccount::writable(stake_pool.address()),
        InstructionAccount::writable(validator_stake_list.address()),
        InstructionAccount::readonly(deposit_stake_authority.address()),
        InstructionAccount::readonly_signer(base.address()),
        InstructionAccount::readonly(stake_pool_withdraw_authority.address()),
        InstructionAccount::writable(stake.address()),
        InstructionAccount::writable(validator_stake_account.address()),
        InstructionAccount::writable(reserve_stake_account.address()),
        InstructionAccount::writable(vault.address()),
        InstructionAccount::writable(manager_fee_account.address()),
        InstructionAccount::writable(referrer_pool_tokens_account.address()),
        InstructionAccount::writable(pool_mint.address()),
        InstructionAccount::readonly(clock.address()),
        InstructionAccount::readonly(stake_history.address()),
        InstructionAccount::readonly(token_program.address()),
        InstructionAccount::readonly(stake_program.address()),
        InstructionAccount::readonly(system_program.address()),
    ];

    let ix = InstructionView {
        program_id: interceptor_program_id,
        accounts: &metas,
        data: &data,
    };

    invoke(
        &ix,
        &[
            payer,
            stake_pool_program,
            deposit_receipt,
            stake_pool,
            validator_stake_list,
            deposit_stake_authority,
            base,
            stake_pool_withdraw_authority,
            stake,
            validator_stake_account,
            reserve_stake_account,
            vault,
            manager_fee_account,
            referrer_pool_tokens_account,
            pool_mint,
            clock,
            stake_history,
            token_program,
            stake_program,
            system_program,
        ],
    )
}
