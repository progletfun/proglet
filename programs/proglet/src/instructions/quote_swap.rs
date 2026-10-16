use anchor_lang::prelude::*;
use proglet_math::swap;

use crate::{constants::*, error::ErrorCode, state::Pool};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SwapQuote {
    pub amount_out: u64,
    pub fee: u64,
    pub team_fee: u64,
    pub price_impact_bps: u64,
    pub spot_price_q64: u128,
    pub sol_reserve_after: u64,
    pub token_reserve_after: u64,
}

#[derive(Accounts)]
pub struct QuoteSwap<'info> {
    #[account(seeds = [POOL_SEED, pool.mint.as_ref()], bump = pool.bump)]
    pub pool: Account<'info, Pool>,
}

pub fn handle_quote_swap(
    ctx: Context<QuoteSwap>,
    is_buy: bool,
    amount_in: u64,
) -> Result<SwapQuote> {
    let pool = &ctx.accounts.pool;
    require!(
        pool.sol_reserve > 0 && pool.token_reserve > 0,
        ErrorCode::EmptyPool
    );
    let (s, t) = (pool.sol_reserve, pool.token_reserve);
    let r = if is_buy {
        swap::buy(s, t, amount_in, SWAP_FEE_BPS, TEAM_SHARE_BPS)
    } else {
        swap::sell(s, t, amount_in, SWAP_FEE_BPS, TEAM_SHARE_BPS)
    }
    .ok_or(ErrorCode::InvalidSwapAmount)?;
    Ok(SwapQuote {
        amount_out: r.amount_out,
        fee: r.fee,
        team_fee: r.team_fee,
        price_impact_bps: swap::price_impact_bps(s, t, is_buy, amount_in, r.amount_out),
        spot_price_q64: pool.spot_price_q64().ok_or(ErrorCode::EmptyPool)?,
        sol_reserve_after: r.sol_reserve_after,
        token_reserve_after: r.token_reserve_after,
    })
}
