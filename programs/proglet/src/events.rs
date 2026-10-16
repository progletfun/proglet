use anchor_lang::prelude::*;

#[event]
pub struct TokenCreated {
    pub mint: Pubkey,
    pub pool: Pubkey,
    pub creator: Pubkey,
    pub user: Pubkey,
}

#[event]
pub struct LiquidityAdded {
    pub pool: Pubkey,
    pub sol_amount: u64,
    pub token_amount: u64,
    pub sol_reserve: u64,
    pub token_reserve: u64,
}

#[event]
pub struct Swapped {
    pub pool: Pubkey,
    pub user: Pubkey,
    pub is_buy: bool,
    pub amount_in: u64,
    pub amount_out: u64,
    pub fee: u64,
    pub team_fee: u64,
    pub sol_reserve: u64,
    pub token_reserve: u64,
}

#[event]
pub struct Swept {
    pub pool: Pubkey,
    pub amount: u64,
}

#[event]
pub struct BuybackExecuted {
    pub pool: Pubkey,
    pub sol_spent: u64,
    pub tokens_bought: u64,
}

#[event]
pub struct Borrowed {
    pub pool: Pubkey,
    pub owner: Pubkey,
    pub collateral: u64,
    pub debt: u64,
    pub fee: u64,
    pub fee_bps: u64,
    pub loan_reserve_token: u64,
}

#[event]
pub struct Repaid {
    pub pool: Pubkey,
    pub owner: Pubkey,
    pub debt: u64,
    pub collateral: u64,
}

#[event]
pub struct PartiallyRepaid {
    pub pool: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub collateral: u64,
    pub remaining_debt: u64,
    pub remaining_collateral: u64,
}

#[event]
pub struct Liquidated {
    pub pool: Pubkey,
    pub owner: Pubkey,
    pub liquidator: Pubkey,
    pub debt: u64,
    pub tokens_to_liquidator: u64,
    pub bonus_bps: u64,
}

#[event]
pub struct Absorbed {
    pub pool: Pubkey,
    pub owner: Pubkey,
    pub bad_debt: u64,
    pub tokens_to_pool: u64,
}
