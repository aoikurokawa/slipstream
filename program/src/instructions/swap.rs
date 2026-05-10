use pinocchio::{
    cpi::{invoke, invoke_signed, Seed, Signer},
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address,
};
use pinocchio_system::instructions::CreateAccount;
use pinocchio_token::state::TokenAccount;

use crate::error::SlipstreamError;

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

/// Routes an LST → LST swap by burning pool A's tokens, splitting a stake
/// account, transferring its authorities to pool B's deposit authority, and
/// minting pool B's tokens to the user. The route is NAV-true modulo the two
/// pools' withdraw/deposit fees.
///
/// # Accounts
///
///  0. `[signer, writable]`  User (payer + LST_A transfer authority).
///  1. `[writable]`          User's LST_A token account.
///  2. `[writable]`          User's LST_B token account.
///  3. `[writable]`          Transient stake PDA (will be created).
///  4. `[]`                  Router authority PDA (becomes transient stake's
///                           staker/withdrawer post-`WithdrawStake`).
///  5. `[]`                  Pool A's stake-pool program.
///  6. `[writable]`          Pool A state account.
///  7. `[writable]`          Pool A validator list.
///  8. `[]`                  Pool A withdraw authority (PDA of pool A program).
///  9. `[writable]`          Pool A validator stake account to split from.
/// 10. `[writable]`          Pool A manager fee token account.
/// 11. `[writable]`          Pool A pool mint (LST_A).
/// 12. `[]`                  Pool B's stake-pool program.
/// 13. `[writable]`          Pool B state account.
/// 14. `[writable]`          Pool B validator list.
/// 15. `[]`                  Pool B deposit authority. Marked as signer iff
///                           the pool uses a custom (non-PDA) deposit auth.
/// 16. `[]`                  Pool B withdraw authority (PDA of pool B program).
/// 17. `[writable]`          Pool B validator stake account for the same
///                           validator that backed the source split.
/// 18. `[writable]`          Pool B reserve stake account.
/// 19. `[writable]`          Pool B manager fee token account.
/// 20. `[writable]`          Pool B referral fee token account.
/// 21. `[writable]`          Pool B pool mint (LST_B).
/// 22. `[]`                  Stake program.
/// 23. `[]`                  Token program.
/// 24. `[]`                  System program.
/// 25. `[]`                  Clock sysvar.
/// 26. `[]`                  Stake history sysvar.
///
/// # Instruction Data (after tag byte)
///
/// | Offset | Size | Field          |
/// |--------|------|----------------|
/// | 0      | 8    | amount_in      |
/// | 8      | 8    | min_amount_out |
/// | 16     | 8    | nonce          |
pub fn process_swap(
    program_id: &Address,
    accounts: &[AccountView],
    amount_in: u64,
    min_amount_out: u64,
    nonce: u64,
) -> Result<(), ProgramError> {
    let [user, user_lst_a, user_lst_b, transient_stake, router_authority, pool_a_program, pool_a, pool_a_validator_list, pool_a_withdraw_authority, pool_a_validator_stake, pool_a_manager_fee, pool_a_mint, pool_b_program, pool_b, pool_b_validator_list, pool_b_deposit_authority, pool_b_withdraw_authority, pool_b_validator_stake, pool_b_reserve_stake, pool_b_manager_fee, pool_b_referral_fee, pool_b_mint, stake_program, token_program, system_program, clock, stake_history] =
        accounts
    else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !user.is_signer() {
        pinocchio_log::log!("user is not a signer");
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

    // Verify transient stake and router authority PDAs.
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

    // Snapshot LST_B balance before the route to slippage-check at the end.
    let pre_balance_b = TokenAccount::from_account_view(user_lst_b)?.amount();

    // ---- Step 1: allocate the transient stake account as a PDA. ----
    let rent = Rent::get()?;
    let transient_lamports = rent.minimum_balance_unchecked(STAKE_ACCOUNT_SIZE as usize);

    let transient_bump_arr = [transient_bump];
    let transient_seeds = [
        Seed::from(TRANSIENT_STAKE_SEED),
        Seed::from(user_addr_bytes),
        Seed::from(nonce_bytes.as_slice()),
        Seed::from(transient_bump_arr.as_slice()),
    ];
    let transient_signer = Signer::from(&transient_seeds);

    CreateAccount {
        from: user,
        to: transient_stake,
        lamports: transient_lamports,
        space: STAKE_ACCOUNT_SIZE,
        owner: &STAKE_PROGRAM_ID,
    }
    .invoke_signed(&[transient_signer])?;

    // ---- Step 2: WithdrawStake from pool A → transient_stake. ----
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

    // ---- Step 3: Authorize router → pool B deposit authority (Staker, then Withdrawer). ----
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

    // ---- Step 4: DepositStake into pool B → mints LST_B to user. ----
    deposit_stake(
        pool_b_program.address(),
        pool_b,
        pool_b_validator_list,
        pool_b_deposit_authority,
        pool_b_withdraw_authority,
        transient_stake,
        pool_b_validator_stake,
        pool_b_reserve_stake,
        user_lst_b,
        pool_b_manager_fee,
        pool_b_referral_fee,
        pool_b_mint,
        clock,
        stake_history,
        token_program,
        stake_program,
    )?;

    // ---- Step 5: slippage check. ----
    let post_balance_b = TokenAccount::from_account_view(user_lst_b)?.amount();
    let received = post_balance_b
        .checked_sub(pre_balance_b)
        .ok_or(SlipstreamError::ArithmeticError)?;
    if received < min_amount_out {
        pinocchio_log::log!("slippage exceeded");
        return Err(SlipstreamError::SlippageExceeded.into());
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn withdraw_stake(
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

#[allow(clippy::too_many_arguments)]
fn deposit_stake(
    pool_program_id: &Address,
    pool: &AccountView,
    validator_list: &AccountView,
    deposit_authority: &AccountView,
    pool_withdraw_authority: &AccountView,
    stake_to_deposit: &AccountView,
    validator_stake: &AccountView,
    reserve_stake: &AccountView,
    user_pool_tokens: &AccountView,
    manager_fee: &AccountView,
    referral_fee: &AccountView,
    pool_mint: &AccountView,
    clock: &AccountView,
    stake_history: &AccountView,
    token_program: &AccountView,
    stake_program: &AccountView,
) -> Result<(), ProgramError> {
    let data = [TAG_DEPOSIT_STAKE];

    // Custom (non-default) deposit authorities must sign the outer tx; the
    // runtime then propagates the signature when we mark `is_signer` here.
    // Default PDA deposit authorities are signed by the stake-pool program
    // internally, so `is_signer = false` is correct in that case.
    let deposit_auth_is_signer = deposit_authority.is_signer();

    let metas = [
        InstructionAccount::writable(pool.address()),
        InstructionAccount::writable(validator_list.address()),
        InstructionAccount::new(deposit_authority.address(), false, deposit_auth_is_signer),
        InstructionAccount::readonly(pool_withdraw_authority.address()),
        InstructionAccount::writable(stake_to_deposit.address()),
        InstructionAccount::writable(validator_stake.address()),
        InstructionAccount::writable(reserve_stake.address()),
        InstructionAccount::writable(user_pool_tokens.address()),
        InstructionAccount::writable(manager_fee.address()),
        InstructionAccount::writable(referral_fee.address()),
        InstructionAccount::writable(pool_mint.address()),
        InstructionAccount::readonly(clock.address()),
        InstructionAccount::readonly(stake_history.address()),
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
            deposit_authority,
            pool_withdraw_authority,
            stake_to_deposit,
            validator_stake,
            reserve_stake,
            user_pool_tokens,
            manager_fee,
            referral_fee,
            pool_mint,
            clock,
            stake_history,
            token_program,
            stake_program,
        ],
    )
}

fn stake_authorize(
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
