use anchor_lang::{prelude::*, system_program};
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};
use proglet_math::swap;

use crate::{
    constants::*,
    error::ErrorCode,
    events::Swapped,
    state::{Pool, TeamFeeVault},
};

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, seeds = [POOL_SEED, mint.key().as_ref()], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,
    #[account(mut, seeds = [TEAM_FEE_SEED, pool.key().as_ref()], bump = pool.team_fee_bump)]
    pub team_fee_vault: Box<Account<'info, TeamFeeVault>>,
    #[account(address = pool.mint)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pool,
        associated_token::token_program = token_program
    )]
    pub pool_token_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, token::authority = user, token::token_program = token_program)]
    pub user_token_account: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = pool.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_swap(
    mut ctx: Context<Swap>,
    is_buy: bool,
    amount_in: u64,
    min_out: u64,
) -> Result<()> {
    require!(
        ctx.accounts.pool.sol_reserve > 0 && ctx.accounts.pool.token_reserve > 0,
        ErrorCode::EmptyPool
    );
    let slot = Clock::get()?.slot;
    ctx.accounts.pool.record_observation(slot);

    let result = if is_buy {
        buy(&ctx, amount_in)?
    } else {
        sell(&mut ctx, amount_in)?
    };
    require!(result.amount_out >= min_out, ErrorCode::SlippageExceeded);

    let pool_info = ctx.accounts.pool.to_account_info();
    if is_buy {
        let mint_key = ctx.accounts.mint.key();
        let seeds: &[&[u8]] = &[POOL_SEED, mint_key.as_ref(), &[ctx.accounts.pool.bump]];
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                TransferChecked {
                    from: ctx.accounts.pool_token_vault.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: pool_info.clone(),
                },
                &[seeds],
            ),
            result.amount_out,
            ctx.accounts.mint.decimals,
        )?;
    } else {
        let user_info = ctx.accounts.user.to_account_info();
        **pool_info.try_borrow_mut_lamports()? -= result.amount_out;
        **user_info.try_borrow_mut_lamports()? += result.amount_out;
    }
    let team_info = ctx.accounts.team_fee_vault.to_account_info();
    **pool_info.try_borrow_mut_lamports()? -= result.team_fee;
    **team_info.try_borrow_mut_lamports()? += result.team_fee;

    let pool = &mut ctx.accounts.pool;
    pool.sol_reserve = result.sol_reserve_after;
    pool.token_reserve = result.token_reserve_after;
    pool.last_swap_slot = slot;

    emit!(Swapped {
        pool: pool.key(),
        user: ctx.accounts.user.key(),
        is_buy,
        amount_in,
        amount_out: result.amount_out,
        fee: result.fee,
        team_fee: result.team_fee,
        sol_reserve: pool.sol_reserve,
        token_reserve: pool.token_reserve,
    });
    Ok(())
}

fn buy(ctx: &Context<Swap>, sol_in: u64) -> Result<swap::SwapResult> {
    let pool = &ctx.accounts.pool;
    let result = swap::buy(
        pool.sol_reserve,
        pool.token_reserve,
        sol_in,
        SWAP_FEE_BPS,
        TEAM_SHARE_BPS,
    )
    .ok_or(ErrorCode::InvalidSwapAmount)?;
    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: ctx.accounts.pool.to_account_info(),
            },
        ),
        sol_in,
    )?;
    Ok(result)
}

fn sell(ctx: &mut Context<Swap>, token_in: u64) -> Result<swap::SwapResult> {
    let vault_before = ctx.accounts.pool_token_vault.amount;
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.user_token_account.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.pool_token_vault.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        token_in,
        ctx.accounts.mint.decimals,
    )?;
    ctx.accounts.pool_token_vault.reload()?;
    let received = ctx
        .accounts
        .pool_token_vault
        .amount
        .checked_sub(vault_before)
        .ok_or(ErrorCode::MathOverflow)?;
    let pool = &ctx.accounts.pool;
    swap::sell(
        pool.sol_reserve,
        pool.token_reserve,
        received,
        SWAP_FEE_BPS,
        TEAM_SHARE_BPS,
    )
    .ok_or(ErrorCode::InvalidSwapAmount.into())
}
