//! Slipstream CLI library — builds the on-chain `Swap` instruction.
//!
//! Mirrors the account ordering documented in
//! `program/src/instructions/swap.rs`. The companion binary loads pool
//! addresses from a JSON config, signs the transaction, and submits it.

use std::str::FromStr;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
};

/// SPL Token program (classic, not Token-2022).
pub const TOKEN_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");

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
