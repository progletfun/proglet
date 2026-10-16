use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct Position {
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub collateral: u64,
    pub debt: u64,
    pub loan_reserve_token: u64,
    pub opened_slot: u64,
    pub bump: u8,
}
