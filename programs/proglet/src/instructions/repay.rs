use anchor_lang::{prelude::*, system_program};
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::{
    constants::*,
    error::ErrorCode,
    events::Repaid,
    state::{Pool, Position},
};

#[derive(Accounts)]
pub struct Repay<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
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
    #[account(mut, token::mint = mint, token::authority = owner, token::token_program = token_program)]
    pub owner_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        mut,
        close = owner,
        seeds = [POSITION_SEED, pool.key().as_ref(), owner.key().as_ref()],
        bump = position.bump,
        has_one = owner,
        has_one = pool
    )]
    pub position: Box<Account<'info, Position>>,
    #[account(address = pool.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_repay(ctx: Context<Repay>) -> Result<()> {
    let position = &ctx.accounts.position;
    let slot = Clock::get()?.slot;
    require!(
        slot >= position.opened_slot.saturating_add(MIN_LOAN_SLOTS),
        ErrorCode::RepayTooEarly
    );
    let (debt, collateral, loan_reserve_token) = (
        position.debt,
        position.collateral,
        position.loan_reserve_token,
    );

    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.owner.to_account_info(),
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
                to: ctx.accounts.owner_token_account.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[seeds],
        ),
        collateral,
        ctx.accounts.mint.decimals,
    )?;

    let pool = &mut ctx.accounts.pool;
    pool.total_debt -= debt;
    pool.total_loan_reserve_token -= loan_reserve_token;
    pool.total_collateral -= collateral;
    pool.return_liquidity(debt, loan_reserve_token);

    emit!(Repaid {
        pool: pool.key(),
        owner: ctx.accounts.owner.key(),
        debt,
        collateral
    });
    Ok(())
}
