use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct TeamFeeVault {
    pub pool: Pubkey,
    pub bump: u8,
}
