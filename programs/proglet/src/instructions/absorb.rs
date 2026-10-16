use anchor_lang::prelude::*;
use proglet_math::lending;

use crate::{
    constants::*,
    error::ErrorCode,
    events::Absorbed,
    state::{Pool, Position},
};

#[derive(Accounts)]
pub struct Absorb<'info> {
    pub caller: Signer<'info>,
    /// CHECK: the position owner; only receives the position's rent.
    #[account(mut, address = position.owner)]
    pub owner: UncheckedAccount<'info>,
    #[account(mut, seeds = [POOL_SEED, pool.mint.as_ref()], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(
        mut,
        close = owner,
        seeds = [POSITION_SEED, pool.key().as_ref(), position.owner.as_ref()],
        bump = position.bump,
        has_one = pool
    )]
    pub position: Box<Account<'info, Position>>,
}

pub fn handle_absorb(ctx: Context<Absorb>) -> Result<()> {
    let pool = &ctx.accounts.pool;
    let position = &ctx.accounts.position;
    let slot = Clock::get()?.slot;
    let safe = pool
        .safe_price_q64(slot, SAFE_PRICE_MIN_AGE_SLOTS)
        .ok_or(ErrorCode::EmptyPool)?;
    let value = pool
        .collateral_value(position.collateral, safe)
        .ok_or(ErrorCode::MathOverflow)?;
    require!(
        lending::is_underwater(value, position.debt),
        ErrorCode::NotUnderwater
    );

    let (debt, collateral, loan_reserve_token) = (
        position.debt,
        position.collateral,
        position.loan_reserve_token,
    );
    let pool = &mut ctx.accounts.pool;
    pool.total_debt -= debt;
    pool.total_loan_reserve_token -= loan_reserve_token;
    pool.total_collateral -= collateral;
    pool.token_reserve += loan_reserve_token + collateral;

    emit!(Absorbed {
        pool: pool.key(),
        owner: ctx.accounts.owner.key(),
        bad_debt: debt,
        tokens_to_pool: loan_reserve_token + collateral,
    });
    Ok(())
}
