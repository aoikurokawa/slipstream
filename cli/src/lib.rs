//! Slipstream CLI library — builds the on-chain `Swap` instruction and
//! materializes its 27-account payload from on-chain stake-pool state.
//!
//! Mirrors the account ordering documented in
//! `program/src/instructions/swap.rs`.

use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

/// SPL Token program (classic, not Token-2022).
pub const TOKEN_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

/// SPL Associated Token Account program.
pub const ASSOCIATED_TOKEN_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

/// Native stake program.
pub const STAKE_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("Stake11111111111111111111111111111111111111");

/// System program.
pub const SYSTEM_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("11111111111111111111111111111111");

/// Clock sysvar.
pub const CLOCK_SYSVAR_ID: Pubkey =
    solana_sdk::pubkey!("SysvarC1ock11111111111111111111111111111111");

/// Stake history sysvar.
pub const STAKE_HISTORY_SYSVAR_ID: Pubkey =
    solana_sdk::pubkey!("SysvarStakeHistory1111111111111111111111111");

/// Canonical SPL stake-pool program. Used by JitoSOL, bSOL, Sanctum LSTs, etc.
/// Marinade has its own program and is NOT compatible without overrides.
pub const SPL_STAKE_POOL_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("SPoo1Ku8WFXoNDMHPsrGSTSG1Y47rzgn41SLUNakuHy");

const TRANSIENT_STAKE_SEED: &[u8] = b"transient";
const ROUTER_AUTHORITY_SEED: &[u8] = b"router";

/// On-chain instruction tag for `SlipstreamInstruction::Swap`.
const TAG_SWAP: u8 = 1;

/// Pool A (source) addresses needed to route a swap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolA {
    pub program: String,
    pub address: String,
    pub validator_list: String,
    pub withdraw_authority: String,
    pub validator_stake: String,
    pub manager_fee: String,
    pub mint: String,
}

/// Pool B (destination) addresses. Note the extra `deposit_authority`,
/// `reserve_stake`, and `referral_fee` accounts vs `PoolA`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolB {
    pub program: String,
    pub address: String,
    pub validator_list: String,
    pub deposit_authority: String,
    pub withdraw_authority: String,
    pub validator_stake: String,
    pub reserve_stake: String,
    pub manager_fee: String,
    pub referral_fee: String,
    pub mint: String,
}

/// Full per-swap configuration, loaded from a JSON file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapConfig {
    pub program_id: String,
    pub user_lst_a: String,
    pub user_lst_b: String,
    pub pool_a: PoolA,
    pub pool_b: PoolB,
}

/// Resolved pubkeys for every account the on-chain `Swap` consumes.
///
/// `user`, `transient_stake`, and `router_authority` are derived from the
/// signer + program ID + nonce, so they're not in the JSON config.
#[derive(Debug, Clone)]
pub struct SwapAccounts {
    pub program_id: Pubkey,
    pub user: Pubkey,
    pub user_lst_a: Pubkey,
    pub user_lst_b: Pubkey,
    pub transient_stake: Pubkey,
    pub router_authority: Pubkey,
    pub pool_a_program: Pubkey,
    pub pool_a: Pubkey,
    pub pool_a_validator_list: Pubkey,
    pub pool_a_withdraw_authority: Pubkey,
    pub pool_a_validator_stake: Pubkey,
    pub pool_a_manager_fee: Pubkey,
    pub pool_a_mint: Pubkey,
    pub pool_b_program: Pubkey,
    pub pool_b: Pubkey,
    pub pool_b_validator_list: Pubkey,
    pub pool_b_deposit_authority: Pubkey,
    pub pool_b_withdraw_authority: Pubkey,
    pub pool_b_validator_stake: Pubkey,
    pub pool_b_reserve_stake: Pubkey,
    pub pool_b_manager_fee: Pubkey,
    pub pool_b_referral_fee: Pubkey,
    pub pool_b_mint: Pubkey,
}

impl SwapAccounts {
    /// Resolve all accounts from a config + the signer's pubkey + a nonce.
    pub fn from_config(cfg: &SwapConfig, user: Pubkey, nonce: u64) -> Result<Self> {
        let program_id = parse_pubkey("program_id", &cfg.program_id)?;
        let (transient_stake, _) = derive_transient_stake(&program_id, &user, nonce);
        let (router_authority, _) = derive_router_authority(&program_id);

        Ok(Self {
            program_id,
            user,
            user_lst_a: parse_pubkey("user_lst_a", &cfg.user_lst_a)?,
            user_lst_b: parse_pubkey("user_lst_b", &cfg.user_lst_b)?,
            transient_stake,
            router_authority,
            pool_a_program: parse_pubkey("pool_a.program", &cfg.pool_a.program)?,
            pool_a: parse_pubkey("pool_a.address", &cfg.pool_a.address)?,
            pool_a_validator_list: parse_pubkey(
                "pool_a.validator_list",
                &cfg.pool_a.validator_list,
            )?,
            pool_a_withdraw_authority: parse_pubkey(
                "pool_a.withdraw_authority",
                &cfg.pool_a.withdraw_authority,
            )?,
            pool_a_validator_stake: parse_pubkey(
                "pool_a.validator_stake",
                &cfg.pool_a.validator_stake,
            )?,
            pool_a_manager_fee: parse_pubkey("pool_a.manager_fee", &cfg.pool_a.manager_fee)?,
            pool_a_mint: parse_pubkey("pool_a.mint", &cfg.pool_a.mint)?,
            pool_b_program: parse_pubkey("pool_b.program", &cfg.pool_b.program)?,
            pool_b: parse_pubkey("pool_b.address", &cfg.pool_b.address)?,
            pool_b_validator_list: parse_pubkey(
                "pool_b.validator_list",
                &cfg.pool_b.validator_list,
            )?,
            pool_b_deposit_authority: parse_pubkey(
                "pool_b.deposit_authority",
                &cfg.pool_b.deposit_authority,
            )?,
            pool_b_withdraw_authority: parse_pubkey(
                "pool_b.withdraw_authority",
                &cfg.pool_b.withdraw_authority,
            )?,
            pool_b_validator_stake: parse_pubkey(
                "pool_b.validator_stake",
                &cfg.pool_b.validator_stake,
            )?,
            pool_b_reserve_stake: parse_pubkey("pool_b.reserve_stake", &cfg.pool_b.reserve_stake)?,
            pool_b_manager_fee: parse_pubkey("pool_b.manager_fee", &cfg.pool_b.manager_fee)?,
            pool_b_referral_fee: parse_pubkey("pool_b.referral_fee", &cfg.pool_b.referral_fee)?,
            pool_b_mint: parse_pubkey("pool_b.mint", &cfg.pool_b.mint)?,
        })
    }
}

/// Build the `Swap` instruction. Account order must match the on-chain
/// destructuring in `process_swap`.
pub fn build_swap_ix(
    accounts: &SwapAccounts,
    amount_in: u64,
    min_amount_out: u64,
    nonce: u64,
) -> Instruction {
    let mut data = Vec::with_capacity(25);
    data.push(TAG_SWAP);
    data.extend_from_slice(&amount_in.to_le_bytes());
    data.extend_from_slice(&min_amount_out.to_le_bytes());
    data.extend_from_slice(&nonce.to_le_bytes());

    let metas = vec![
        AccountMeta::new(accounts.user, true),
        AccountMeta::new(accounts.user_lst_a, false),
        AccountMeta::new(accounts.user_lst_b, false),
        AccountMeta::new(accounts.transient_stake, false),
        AccountMeta::new_readonly(accounts.router_authority, false),
        AccountMeta::new_readonly(accounts.pool_a_program, false),
        AccountMeta::new(accounts.pool_a, false),
        AccountMeta::new(accounts.pool_a_validator_list, false),
        AccountMeta::new_readonly(accounts.pool_a_withdraw_authority, false),
        AccountMeta::new(accounts.pool_a_validator_stake, false),
        AccountMeta::new(accounts.pool_a_manager_fee, false),
        AccountMeta::new(accounts.pool_a_mint, false),
        AccountMeta::new_readonly(accounts.pool_b_program, false),
        AccountMeta::new(accounts.pool_b, false),
        AccountMeta::new(accounts.pool_b_validator_list, false),
        AccountMeta::new_readonly(accounts.pool_b_deposit_authority, false),
        AccountMeta::new_readonly(accounts.pool_b_withdraw_authority, false),
        AccountMeta::new(accounts.pool_b_validator_stake, false),
        AccountMeta::new(accounts.pool_b_reserve_stake, false),
        AccountMeta::new(accounts.pool_b_manager_fee, false),
        AccountMeta::new(accounts.pool_b_referral_fee, false),
        AccountMeta::new(accounts.pool_b_mint, false),
        AccountMeta::new_readonly(STAKE_PROGRAM_ID, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
        AccountMeta::new_readonly(CLOCK_SYSVAR_ID, false),
        AccountMeta::new_readonly(STAKE_HISTORY_SYSVAR_ID, false),
    ];
    debug_assert_eq!(metas.len(), 27);

    Instruction {
        program_id: accounts.program_id,
        accounts: metas,
        data,
    }
}

/// Derive the per-swap transient stake PDA. Must match the on-chain seeds
/// (`["transient", user, nonce]`) byte-for-byte.
pub fn derive_transient_stake(program_id: &Pubkey, user: &Pubkey, nonce: u64) -> (Pubkey, u8) {
    let nonce_bytes = nonce.to_le_bytes();
    Pubkey::find_program_address(
        &[TRANSIENT_STAKE_SEED, user.as_ref(), &nonce_bytes],
        program_id,
    )
}

/// Derive the program's global router authority PDA (`["router"]`).
pub fn derive_router_authority(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[ROUTER_AUTHORITY_SEED], program_id)
}

fn parse_pubkey(field: &str, s: &str) -> Result<Pubkey> {
    Pubkey::from_str(s).with_context(|| format!("invalid pubkey in `{field}`: {s}"))
}

// ============================================================================
// Stake pool state parsing + derive-config
// ============================================================================

/// Subset of `spl_stake_pool::state::StakePool` we need. Layout is borsh-
/// sequential, so we parse by byte offset rather than pulling in
/// `spl-stake-pool` as a heavy dependency.
#[derive(Debug, Clone)]
pub struct StakePoolState {
    pub stake_deposit_authority: Pubkey,
    pub stake_withdraw_bump_seed: u8,
    pub validator_list: Pubkey,
    pub reserve_stake: Pubkey,
    pub pool_mint: Pubkey,
    pub manager_fee_account: Pubkey,
}

impl StakePoolState {
    /// Parse the first 258 bytes of a stake-pool account. Returns an error if
    /// the buffer is too short or the account-type tag isn't `StakePool` (=1).
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 258 {
            return Err(anyhow!(
                "stake pool account is too small ({} bytes, need ≥ 258)",
                data.len()
            ));
        }
        // account_type: 0 = Uninitialized, 1 = StakePool, 2 = ValidatorList.
        if data[0] != 1 {
            return Err(anyhow!(
                "account_type is {} (expected 1 = StakePool)",
                data[0]
            ));
        }

        Ok(Self {
            stake_deposit_authority: read_pubkey(data, 65),
            stake_withdraw_bump_seed: data[97],
            validator_list: read_pubkey(data, 98),
            reserve_stake: read_pubkey(data, 130),
            pool_mint: read_pubkey(data, 162),
            manager_fee_account: read_pubkey(data, 194),
        })
    }
}

fn read_pubkey(data: &[u8], offset: usize) -> Pubkey {
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&data[offset..offset + 32]);
    Pubkey::new_from_array(buf)
}

/// Derive `[pool, "withdraw"]` PDA.
pub fn derive_withdraw_authority(pool_program: &Pubkey, pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[pool.as_ref(), b"withdraw"], pool_program)
}

/// Derive the *primary* per-validator stake account PDA (no transient seed).
pub fn derive_validator_stake(
    pool_program: &Pubkey,
    vote_account: &Pubkey,
    pool: &Pubkey,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[vote_account.as_ref(), pool.as_ref(), &[]], pool_program)
}

/// Derive the associated token account for `(owner, mint)`.
pub fn derive_ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[owner.as_ref(), TOKEN_PROGRAM_ID.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM_ID,
    )
    .0
}

/// Inputs to the config-derivation flow. Anything optional falls back to a
/// sensible default (canonical SPL stake-pool program, `referral_fee` =
/// `manager_fee_account`).
#[derive(Debug, Clone)]
pub struct DeriveInputs {
    pub slipstream_program_id: Pubkey,
    pub pool_a: Pubkey,
    pub pool_b: Pubkey,
    pub validator_vote: Pubkey,
    pub user: Pubkey,
    pub pool_a_program: Option<Pubkey>,
    pub pool_b_program: Option<Pubkey>,
}

/// Fetch both pool states from RPC and assemble a [`SwapConfig`]. The user's
/// LST_A / LST_B token accounts are derived as ATAs of the pools' mints.
pub fn derive_swap_config(rpc: &RpcClient, inputs: &DeriveInputs) -> Result<SwapConfig> {
    let pool_a_program = inputs
        .pool_a_program
        .unwrap_or(SPL_STAKE_POOL_PROGRAM_ID);
    let pool_b_program = inputs
        .pool_b_program
        .unwrap_or(SPL_STAKE_POOL_PROGRAM_ID);

    let pool_a_acct = rpc
        .get_account(&inputs.pool_a)
        .with_context(|| format!("fetch pool A {}", inputs.pool_a))?;
    let pool_b_acct = rpc
        .get_account(&inputs.pool_b)
        .with_context(|| format!("fetch pool B {}", inputs.pool_b))?;

    if pool_a_acct.owner != pool_a_program {
        return Err(anyhow!(
            "pool A owner {} != expected program {}",
            pool_a_acct.owner,
            pool_a_program
        ));
    }
    if pool_b_acct.owner != pool_b_program {
        return Err(anyhow!(
            "pool B owner {} != expected program {}",
            pool_b_acct.owner,
            pool_b_program
        ));
    }

    let pool_a_state = StakePoolState::parse(&pool_a_acct.data).context("parse pool A")?;
    let pool_b_state = StakePoolState::parse(&pool_b_acct.data).context("parse pool B")?;

    let (pool_a_withdraw_authority, _) = derive_withdraw_authority(&pool_a_program, &inputs.pool_a);
    let (pool_b_withdraw_authority, _) = derive_withdraw_authority(&pool_b_program, &inputs.pool_b);
    let (pool_a_validator_stake, _) =
        derive_validator_stake(&pool_a_program, &inputs.validator_vote, &inputs.pool_a);
    let (pool_b_validator_stake, _) =
        derive_validator_stake(&pool_b_program, &inputs.validator_vote, &inputs.pool_b);

    let user_lst_a = derive_ata(&inputs.user, &pool_a_state.pool_mint);
    let user_lst_b = derive_ata(&inputs.user, &pool_b_state.pool_mint);

    Ok(SwapConfig {
        program_id: inputs.slipstream_program_id.to_string(),
        user_lst_a: user_lst_a.to_string(),
        user_lst_b: user_lst_b.to_string(),
        pool_a: PoolA {
            program: pool_a_program.to_string(),
            address: inputs.pool_a.to_string(),
            validator_list: pool_a_state.validator_list.to_string(),
            withdraw_authority: pool_a_withdraw_authority.to_string(),
            validator_stake: pool_a_validator_stake.to_string(),
            manager_fee: pool_a_state.manager_fee_account.to_string(),
            mint: pool_a_state.pool_mint.to_string(),
        },
        pool_b: PoolB {
            program: pool_b_program.to_string(),
            address: inputs.pool_b.to_string(),
            validator_list: pool_b_state.validator_list.to_string(),
            deposit_authority: pool_b_state.stake_deposit_authority.to_string(),
            withdraw_authority: pool_b_withdraw_authority.to_string(),
            validator_stake: pool_b_validator_stake.to_string(),
            reserve_stake: pool_b_state.reserve_stake.to_string(),
            manager_fee: pool_b_state.manager_fee_account.to_string(),
            // No referral relationship by default — point at the manager fee
            // account, which the SPL stake pool program accepts.
            referral_fee: pool_b_state.manager_fee_account.to_string(),
            mint: pool_b_state.pool_mint.to_string(),
        },
    })
}
