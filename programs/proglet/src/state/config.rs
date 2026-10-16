use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct GlobalConfig {
    pub admin: Pubkey,
    pub crank_authority: Pubkey,
    pub treasury: Pubkey,
    pub borrow_paused: bool,
    pub bump: u8,
}
