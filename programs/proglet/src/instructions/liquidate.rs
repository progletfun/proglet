use anchor_lang::{prelude::*, system_program};
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};
use proglet_math::{lending, BPS};

use crate::{
    constants::*,
    error::ErrorCode,
    events::Liquidated,
    state::{Pool, Position},
};

#[derive(Accounts)]
pub struct Liquidate<'info> {
    #[account(mut)]
    pub liquidator: Signer<'info>,
    /// CHECK: the position owner; only receives the position's rent.
    #[account(mut, address = position.owner)]
    pub owner: UncheckedAccount<'info>,
    #[account(mut, seeds = [POOL_SEED, mint.key().as_ref()], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(address = pool.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pool,
        associated_token::token_program = token_program
    )]
    pub pool_token_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, token::token_program = token_program)]
    pub liquidator_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        close = owner,
        seeds = [POSITION_SEED, pool.key().as_ref(), position.owner.as_ref()],
        bump = position.bump,
        has_one = pool
    )]
    pub position: Box<Account<'info, Position>>,
    #[account(address = pool.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_liquidate(ctx: Context<Liquidate>) -> Result<()> {
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
        lending::is_liquidatable(value, position.debt),
        ErrorCode::NotLiquidatable
    );

    let bonus_bps = lending::liquidation_bonus_bps(value, position.debt);
    let owed_value = position.debt as u128 * (BPS + bonus_bps) as u128 / BPS as u128;
    let tokens_to_liquidator = lending::tokens_for_sol(owed_value, safe)
        .ok_or(ErrorCode::MathOverflow)?
        .min(position.collateral);
    let (debt, collateral, loan_reserve_token) = (
        position.debt,
        position.collateral,
        position.loan_reserve_token,
    );

    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.liquidator.to_account_info(),
                to: ctx.accounts.pool.to_account_info(),
            },
        ),
        debt,
    )?;
    let mint_key = ctx.accounts.mint.key();
    let seeds: &[&[u8]] = &[POOL_SEED, mint_key.as_ref(), &[ctx.accounts.pool.bump]];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.pool_token_vault.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.liquidator_token_account.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[seeds],
        ),
        tokens_to_liquidator,
        ctx.accounts.mint.decimals,
    )?;

    let pool = &mut ctx.accounts.pool;
    pool.total_debt -= debt;
    pool.total_loan_reserve_token -= loan_reserve_token;
    pool.total_collateral -= collateral;
    pool.return_liquidity(
        debt,
        loan_reserve_token + (collateral - tokens_to_liquidator),
    );

    emit!(Liquidated {
        pool: pool.key(),
        owner: ctx.accounts.owner.key(),
        liquidator: ctx.accounts.liquidator.key(),
        debt,
        tokens_to_liquidator,
        bonus_bps,
    });
    Ok(())
}
