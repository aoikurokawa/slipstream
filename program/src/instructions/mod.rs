use pinocchio::error::ProgramError;

pub mod initialize_config;
pub mod swap;

#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum SlipstreamInstruction {
    /// Initialize config
    InitializeConfig,

    /// Stake-pool routed swap (LST_A → LST_B via withdraw_stake → deposit_stake).
    Swap {
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
            _ => return Err(ProgramError::InvalidInstructionData),
        })
    }
}
