use anchor_lang::{prelude::*, system_program};
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::{
    constants::*,
    error::ErrorCode,
    events::PartiallyRepaid,
    state::{Pool, Position},
};

#[derive(Accounts)]
pub struct RepayPartial<'info> {
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

pub fn handle_repay_partial(ctx: Context<RepayPartial>, amount: u64) -> Result<()> {
    let position = &ctx.accounts.position;
    let slot = Clock::get()?.slot;
    require!(
        slot >= position.opened_slot.saturating_add(MIN_LOAN_SLOTS),
        ErrorCode::RepayTooEarly
    );
    require!(
        amount > 0 && amount < position.debt,
        ErrorCode::InvalidRepayAmount
    );
    require!(
        position.debt - amount >= MIN_DEBT_LAMPORTS,
        ErrorCode::DebtTooSmall
    );

    let share = |x: u64| (x as u128 * amount as u128 / position.debt as u128) as u64;
    let collateral_out = share(position.collateral);
    let reserve_back = share(position.loan_reserve_token);

    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.owner.to_account_info(),
                to: ctx.accounts.pool.to_account_info(),
            },
        ),
        amount,
    )?;
    if collateral_out > 0 {
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
            collateral_out,
            ctx.accounts.mint.decimals,
        )?;
    }

    let pool = &mut ctx.accounts.pool;
    pool.total_debt -= amount;
    pool.total_loan_reserve_token -= reserve_back;
    pool.total_collateral -= collateral_out;
    pool.return_liquidity(amount, reserve_back);

    let position = &mut ctx.accounts.position;
    position.debt -= amount;
    position.collateral -= collateral_out;
    position.loan_reserve_token -= reserve_back;

    emit!(PartiallyRepaid {
        pool: pool.key(),
        owner: position.owner,
        amount,
        collateral: collateral_out,
        remaining_debt: position.debt,
        remaining_collateral: position.collateral,
    });
    Ok(())
}
