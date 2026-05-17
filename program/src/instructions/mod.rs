use pinocchio::error::ProgramError;
use shank::ShankInstruction;

pub mod initialize_config;
pub mod route_common;
pub mod swap;
pub mod swap_via_interceptor;

#[derive(Clone, Debug, PartialEq, Eq, ShankInstruction)]
#[repr(C)]
pub enum SlipstreamInstruction {
    /// Initialize config
    #[account(0, writable, name = "config")]
    #[account(1, name = "payer")]
    #[account(2, name = "system_program")]
    InitializeConfig,

    /// Stake-pool routed swap (LST_A → LST_B via withdraw_stake → deposit_stake).
    #[account(0, name = "user")]
    #[account(1, name = "user_lst_a")]
    #[account(2, name = "user_lst_b")]
    #[account(3, name = "transient_stake")]
    #[account(4, name = "router_authority")]
    #[account(5, name = "pool_a_program")]
    #[account(6, name = "pool_a")]
    #[account(7, name = "pool_a_validator_list")]
    #[account(8, name = "pool_a_withdraw_authority")]
    #[account(9, name = "pool_a_validator_stake")]
    #[account(10, name = "pool_a_manager_fee")]
    #[account(11, name = "pool_a_mint")]
    #[account(12, name = "pool_b_program")]
    #[account(13, name = "pool_b")]
    #[account(14, name = "pool_b_validator_list")]
    #[account(15, name = "pool_b_deposit_authority")]
    #[account(16, name = "pool_b_deposit_authority")]
    #[account(17, name = "pool_b_withdraw_authority")]
    #[account(18, name = "pool_b_validator_stake")]
    #[account(19, name = "pool_b_reserve_stake")]
    #[account(20, name = "pool_b_manager_fee")]
    #[account(21, name = "pool_b_referral_fee")]
    #[account(22, name = "pool_b_mint")]
    #[account(23, name = "stake_program")]
    #[account(24, name = "token_program")]
    #[account(25, name = "system_program")]
    #[account(26, name = "clock")]
    #[account(27, name = "stake_history")]
    Swap {
        /// Amount of LST_A to burn at pool A.
        amount_in: u64,

        /// Minimum LST_B to receive from pool B; reverts on slippage.
        min_amount_out: u64,

        /// Nonce that distinguishes the per-swap transient stake PDA. Lets a
        /// caller route concurrent swaps without colliding on the PDA.
        nonce: u64,
    },

    /// Stake-pool routed swap when pool B's deposit authority is owned by
    /// Jito's stake-deposit-interceptor program. Same in/out shape as `Swap`
    /// but routes the deposit through `interceptor::DepositStake` +
    /// `interceptor::ClaimPoolTokens` in the same transaction.
    #[account(0, name = "user")]
    #[account(1, name = "user_lst_a")]
    #[account(2, name = "transient_stake")]
    #[account(3, name = "router_authority")]
    #[account(4, name = "pool_a_program")]
    #[account(5, name = "pool_a")]
    #[account(6, name = "pool_a_validator_list")]
    #[account(7, name = "pool_a_withdraw_authority")]
    #[account(8, name = "pool_a_validator_stake")]
    #[account(9, name = "pool_a_manager_fee")]
    #[account(10, name = "pool_a_mint")]
    #[account(11, name = "pool_b_program")]
    #[account(12, name = "pool_b")]
    #[account(13, name = "pool_b_validator_list")]
    #[account(14, name = "pool_b_deposit_authority")]
    #[account(15, name = "pool_b_withdraw_authority")]
    #[account(16, name = "pool_b_validator_stake")]
    #[account(17, name = "pool_b_reserve_stake")]
    #[account(18, name = "pool_b_manager_fee")]
    #[account(19, name = "pool_b_referral_fee")]
    #[account(20, name = "pool_b_mint")]
    #[account(21, name = "stake_program")]
    #[account(22, name = "token_program")]
    #[account(23, name = "system_program")]
    #[account(24, name = "clock")]
    #[account(25, name = "stake_history")]
    #[account(26, name = "interceptor_program")]
    #[account(27, name = "deposit_receipt")]
    #[account(28, name = "base")]
    #[account(29, name = "vault")]
    SwapViaInterceptor {
        /// Amount of LST_A to burn at pool A.
        amount_in: u64,

        /// Minimum LST_B to receive from pool B; reverts on slippage.
        min_amount_out: u64,

        /// Nonce that distinguishes the per-swap transient stake PDA. Lets a
        /// caller route concurrent swaps without colliding on the PDA.
        nonce: u64,
    },
}

impl SlipstreamInstruction {
    pub fn unpack(input: &[u8]) -> Result<Self, ProgramError> {
        let (tag, rest) = input
            .split_first()
            .ok_or(ProgramError::InvalidInstructionData)?;

        Ok(match tag {
            0 => Self::InitializeConfig,
            1 => {
                if rest.len() < 24 {
                    return Err(ProgramError::InvalidInstructionData);
                }
                let amount_in = u64::from_le_bytes(rest[0..8].try_into().unwrap());
                let min_amount_out = u64::from_le_bytes(rest[8..16].try_into().unwrap());
                let nonce = u64::from_le_bytes(rest[16..24].try_into().unwrap());

                Self::Swap {
                    amount_in,
                    min_amount_out,
                    nonce,
                }
            }
            2 => {
                if rest.len() < 24 {
                    return Err(ProgramError::InvalidInstructionData);
                }
                let amount_in = u64::from_le_bytes(rest[0..8].try_into().unwrap());
                let min_amount_out = u64::from_le_bytes(rest[8..16].try_into().unwrap());
                let nonce = u64::from_le_bytes(rest[16..24].try_into().unwrap());

                Self::SwapViaInterceptor {
                    amount_in,
                    min_amount_out,
                    nonce,
                }
            }
            _ => return Err(ProgramError::InvalidInstructionData),
        })
    }
}
